// ═══════════════════════════════════════════════════════════════════════════════
// w767_init — PID 1 for the w767-os Phase-2 initramfs.
//
// Responsibilities (in order):
//   1. Install an on-screen bootlog painter (so the eDP framebuffer shows
//      progress during boot; we have no serial console on the W767).
//   2. Mount pseudo-filesystems: /proc, /sys, /dev (devtmpfs), /dev/pts,
//      /run (tmpfs), /tmp (tmpfs).
//   3. Populate /dev with `busybox mdev -s`, then register mdev as the
//      kobject hotplug handler.
//   4. Boot the static daemon DAG:
//         Phase A: spawn w767_netd_lite, wait for READY via W767_NOTIFY_FD.
//         Phase B: spawn dropbear + w767_ctl in parallel.
//   5. Reap zombies + supervise with exponential-backoff restarts forever.
//
// This is a pared-down fork of sol_init — DAG, pipe readiness + epoll,
// supervisor, console logging are kept; the SolOS-specific compositor
// wiring (sol_panel / sol_windowd / sol_bridge / coral-mount / audiod /
// notifyd / maild / matrixd / credstore / telemetryd) is removed entirely.
// Paths retargeted to /opt/w767/bin, /etc/w767, /run/w767.
// ═══════════════════════════════════════════════════════════════════════════════

use anyhow::Result;
use nix::mount::{mount, MsFlags};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::info;
use w767_paths::{W767_BIN, W767_RUN};

mod bootlog;

// ---------------------------------------------------------------------------
// DAG
// ---------------------------------------------------------------------------

struct DaemonDef {
    name: &'static str,
    path: &'static str,
    args: &'static [&'static str],
    depends_on: &'static [&'static str],
    /// Whether we wait for a READY signal on W767_NOTIFY_FD before marking
    /// this daemon "ready". Third-party binaries (dropbear) don't implement
    /// the protocol — we mark them ready immediately after a successful exec.
    notifies_ready: bool,
    max_restarts: u32,
}

const DAEMONS: &[DaemonDef] = &[
    DaemonDef {
        name: "w767_netd",
        path: "/opt/w767/bin/w767_netd_lite",
        args: &[],
        depends_on: &[],
        notifies_ready: true,
        max_restarts: 5,
    },
    DaemonDef {
        name: "dropbear",
        path: "/opt/w767/bin/dropbear",
        // -F foreground, -E stderr, -R generate host keys on demand,
        // -w disallow root login by password, -s disable password auth.
        args: &["-F", "-E", "-s", "-w", "-R"],
        depends_on: &["w767_netd"],
        notifies_ready: false,
        max_restarts: 5,
    },
    DaemonDef {
        name: "w767_ctl",
        path: "/opt/w767/bin/w767_ctl",
        args: &[],
        depends_on: &["w767_netd"],
        notifies_ready: true,
        max_restarts: 5,
    },
];

// ---------------------------------------------------------------------------
// Multi-channel logging
// ---------------------------------------------------------------------------

fn console_log(msg: &str) {
    // /dev/console — visible on VT + serial if cmdline has one
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/console") {
        let _ = writeln!(f, "[w767_init] {msg}");
    }
    // /dev/kmsg — shows up in dmesg
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/kmsg") {
        let _ = writeln!(f, "w767_init: {msg}");
    }
    // persistent log (for post-mortem from the control daemon)
    let _ = std::fs::create_dir_all("/var/log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/var/log/w767_init.log")
    {
        let _ = writeln!(f, "[w767_init] {msg}");
    }
    // on-screen framebuffer
    bootlog::paint(msg);
    info!("{}", msg);
}

// ---------------------------------------------------------------------------
// FS bring-up
// ---------------------------------------------------------------------------

fn ensure_dir(path: &str) {
    let _ = std::fs::create_dir_all(path);
}

fn mount_one(src: &str, dst: &str, fstype: &str, flags: MsFlags) {
    ensure_dir(dst);
    match mount(Some(src), dst, Some(fstype), flags, None::<&str>) {
        Ok(_) => console_log(&format!("[  OK  ] mount {fstype} → {dst}")),
        Err(nix::errno::Errno::EBUSY) => {
            // Already mounted (common on initramfs-as-rootfs). Not an error.
            console_log(&format!("[ INFO ] {fstype} already mounted at {dst}"));
        }
        Err(e) => console_log(&format!("[ WARN ] mount {fstype}→{dst} failed: {e}")),
    }
}

fn mount_filesystems() {
    mount_one("proc",     "/proc",     "proc",     MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC | MsFlags::MS_NODEV);
    mount_one("sysfs",    "/sys",      "sysfs",    MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC | MsFlags::MS_NODEV);
    mount_one("devtmpfs", "/dev",      "devtmpfs", MsFlags::MS_NOSUID);
    mount_one("devpts",   "/dev/pts",  "devpts",   MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC);
    mount_one("tmpfs",    "/run",      "tmpfs",    MsFlags::MS_NOSUID | MsFlags::MS_NODEV);
    mount_one("tmpfs",    "/tmp",      "tmpfs",    MsFlags::MS_NOSUID | MsFlags::MS_NODEV);
    ensure_dir(W767_RUN);
}

fn setup_mdev() {
    console_log("[ INFO ] Populating /dev via busybox mdev -s");
    let busybox = format!("{W767_BIN}/busybox");
    let ok = std::process::Command::new(&busybox)
        .args(["mdev", "-s"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        console_log("[ WARN ] mdev -s failed or not present — devtmpfs alone should suffice for most devices");
    }
    // Register mdev as the kobject hotplug handler.
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/proc/sys/kernel/hotplug") {
        let _ = write!(f, "{busybox}\n");
        console_log("[  OK  ] hotplug handler registered");
    }
}

// ---------------------------------------------------------------------------
// Daemon spawn with readiness pipe
// ---------------------------------------------------------------------------

fn spawn_with_notify(def: &DaemonDef) -> Option<(std::process::Child, Option<RawFd>)> {
    // If the daemon doesn't speak the notify protocol, just spawn it plain.
    if !def.notifies_ready {
        return spawn_plain(def).map(|c| (c, None));
    }

    // Create a pipe. Write end goes to the child (via W767_NOTIFY_FD); read
    // end stays with us for epoll.
    let mut pipe_fds = [0i32; 2];
    // SAFETY: pipe_fds is a valid 2-element array; pipe() fills both ends atomically.
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } != 0 {
        console_log(&format!("[ FAIL ] pipe() failed for {}", def.name));
        return None;
    }
    let read_fd = pipe_fds[0];
    let write_fd = pipe_fds[1];

    // Read end non-blocking (epoll edge-triggered safety).
    // SAFETY: read_fd is a valid pipe FD from pipe() above.
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFL);
        libc::fcntl(read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    let mut cmd = std::process::Command::new(def.path);
    cmd.args(def.args)
        .env(sol_ipc_notify_env(), write_fd.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    // The child inherits write_fd; we close it in the parent after fork so
    // the pipe hits EOF if the daemon crashes before signalling.
    match cmd.spawn() {
        Ok(child) => {
            // SAFETY: write_fd was created above and is ours to close.
            unsafe { libc::close(write_fd); }
            console_log(&format!("[ INFO ] {} started (pid {})", def.name, child.id()));
            Some((child, Some(read_fd)))
        }
        Err(e) => {
            // SAFETY: both ends are ours on spawn failure; close to avoid leak.
            unsafe { libc::close(read_fd); libc::close(write_fd); }
            console_log(&format!("[ FAIL ] spawn {}: {}", def.name, e));
            None
        }
    }
}

fn spawn_plain(def: &DaemonDef) -> Option<std::process::Child> {
    let mut cmd = std::process::Command::new(def.path);
    cmd.args(def.args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    match cmd.spawn() {
        Ok(child) => {
            console_log(&format!("[ INFO ] {} started (pid {}, no ready-signal)", def.name, child.id()));
            Some(child)
        }
        Err(e) => {
            console_log(&format!("[ FAIL ] spawn {}: {}", def.name, e));
            None
        }
    }
}

/// Hardcoded notify env var name — vendored sol_ipc has the same constant,
/// but we don't depend on sol_ipc here to keep w767_init's dependency tree
/// minimal.
fn sol_ipc_notify_env() -> &'static str { "W767_NOTIFY_FD" }

// ---------------------------------------------------------------------------
// DAG boot with epoll readiness
// ---------------------------------------------------------------------------

fn boot_daemons() -> HashMap<&'static str, std::process::Child> {
    let mut ready: HashSet<&str> = HashSet::new();
    let mut running: HashMap<&str, std::process::Child> = HashMap::new();
    let mut fd_to_name: HashMap<RawFd, &str> = HashMap::new();

    // SAFETY: no args validated; EPOLL_CLOEXEC prevents FD leak across exec.
    let epoll_fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
    if epoll_fd < 0 {
        console_log("[ FAIL ] epoll_create1 failed — falling back to sequential boot");
        // Fallback: spawn everything, don't wait for readiness.
        for def in DAEMONS {
            if let Some((child, _)) = spawn_with_notify(def) {
                running.insert(def.name, child);
            }
        }
        return running;
    }

    let add_to_epoll = |epoll_fd: RawFd, read_fd: RawFd| {
        let mut event = libc::epoll_event {
            events: (libc::EPOLLIN | libc::EPOLLHUP) as u32,
            u64: read_fd as u64,
        };
        // SAFETY: epoll_fd and read_fd are valid open fds.
        unsafe { libc::epoll_ctl(epoll_fd, libc::EPOLL_CTL_ADD, read_fd, &mut event); }
    };

    // Spawn everything with no dependencies right away.
    for def in DAEMONS {
        if def.depends_on.is_empty() {
            if let Some((child, maybe_fd)) = spawn_with_notify(def) {
                running.insert(def.name, child);
                if let Some(fd) = maybe_fd {
                    add_to_epoll(epoll_fd, fd);
                    fd_to_name.insert(fd, def.name);
                } else {
                    // No readiness protocol — assume ready.
                    ready.insert(def.name);
                }
            }
        }
    }

    let boot_start = Instant::now();
    let boot_timeout = Duration::from_secs(30);

    loop {
        // Spawn newly-unblocked daemons.
        for def in DAEMONS {
            if running.contains_key(def.name) || !def.depends_on.iter().all(|d| ready.contains(d)) {
                continue;
            }
            if let Some((child, maybe_fd)) = spawn_with_notify(def) {
                running.insert(def.name, child);
                if let Some(fd) = maybe_fd {
                    add_to_epoll(epoll_fd, fd);
                    fd_to_name.insert(fd, def.name);
                } else {
                    ready.insert(def.name);
                }
            }
        }

        // All done?
        if DAEMONS.iter().all(|d| ready.contains(d.name)) {
            console_log(&format!("[  OK  ] All daemons ready in {:.1}s", boot_start.elapsed().as_secs_f64()));
            break;
        }

        let remaining = boot_timeout.saturating_sub(boot_start.elapsed());
        if remaining.is_zero() {
            let missing: Vec<&str> = DAEMONS.iter().map(|d| d.name).filter(|n| !ready.contains(n)).collect();
            console_log(&format!("[ WARN ] Boot timed out — still waiting for: {:?}", missing));
            break;
        }

        let timeout_ms = remaining.as_millis().min(5_000) as i32;
        let mut events = [libc::epoll_event { events: 0, u64: 0 }; 8];
        // SAFETY: epoll_fd is valid; events is stack-allocated with known length.
        let n = unsafe {
            libc::epoll_wait(epoll_fd, events.as_mut_ptr(), events.len() as i32, timeout_ms)
        };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EINTR) { continue; }
            console_log(&format!("[ FAIL ] epoll_wait: {e}"));
            break;
        }

        for i in 0..n as usize {
            let fd = events[i].u64 as RawFd;
            let Some(&name) = fd_to_name.get(&fd) else { continue; };
            if events[i].events & libc::EPOLLIN as u32 != 0 {
                let mut buf = [0u8; 16];
                // SAFETY: fd is a valid read-end of a pipe we own.
                let r = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
                if r > 0 && buf[..r as usize].starts_with(b"READY") {
                    ready.insert(name);
                    console_log(&format!("[  OK  ] {} ready", name));
                }
            }
            if events[i].events & libc::EPOLLHUP as u32 != 0 {
                // Daemon closed the pipe without signalling — it crashed or
                // doesn't implement the protocol. Mark ready anyway so the
                // DAG can proceed; the supervisor will restart it if it died.
                if !ready.contains(name) {
                    ready.insert(name);
                    console_log(&format!("[ WARN ] {} notify-pipe EOF without READY (assumed ready)", name));
                }
            }
        }
    }

    // SAFETY: epoll_fd is ours.
    unsafe { libc::close(epoll_fd); }
    running
}

// ---------------------------------------------------------------------------
// Supervision loop
// ---------------------------------------------------------------------------

fn supervise(name: &'static str, def: &'static DaemonDef) {
    let mut restarts: u32 = 0;
    loop {
        if restarts > def.max_restarts {
            console_log(&format!("[ FAIL ] {name} exceeded max_restarts ({}); giving up", def.max_restarts));
            return;
        }
        // Exponential backoff: 1s, 2s, 4s, …, capped at 60s.
        if restarts > 0 {
            let delay = Duration::from_secs(1u64 << restarts.min(6));
            console_log(&format!("[ INFO ] {name} backoff {}s before restart #{}", delay.as_secs(), restarts));
            std::thread::sleep(delay);
        }

        let child_opt = if def.notifies_ready {
            spawn_with_notify(def).map(|(c, _)| c)
        } else {
            spawn_plain(def)
        };
        let Some(mut child) = child_opt else {
            restarts += 1;
            continue;
        };
        match child.wait() {
            Ok(status) => console_log(&format!("[ WARN ] {name} exited {status}")),
            Err(e)     => console_log(&format!("[ FAIL ] wait({name}): {e}")),
        }
        restarts += 1;
    }
}

fn spawn_supervisors(initial: HashMap<&'static str, std::process::Child>) {
    // Drop the initial Child handles — the supervisor respawns from scratch
    // on the first exit, so we don't need to `wait()` these ourselves.
    // The kernel still reaps them via our SIGCHLD loop below.
    drop(initial);

    for def in DAEMONS {
        let name = def.name;
        std::thread::Builder::new()
            .name(format!("supervise_{name}"))
            .spawn(move || supervise(name, def))
            .ok();
    }
}

// ---------------------------------------------------------------------------
// Zombie reaper (PID 1 must reap adopted processes)
// ---------------------------------------------------------------------------

fn reap_forever() -> ! {
    loop {
        // SAFETY: waitpid(-1, ...) is safe; we ignore status.
        let pid = unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) };
        if pid <= 0 {
            // No zombies right now; sleep briefly to avoid a busy loop.
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn setup_tracing() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

fn main() -> Result<()> {
    setup_tracing();
    bootlog::init();
    console_log("[ INFO ] w767_init starting");

    // Mount pseudo-filesystems.
    mount_filesystems();

    // Populate /dev.
    setup_mdev();

    // Make sure the run dir exists (sockets, netd.ready).
    ensure_dir(W767_RUN);

    // Boot the daemon DAG.
    let initial = boot_daemons();

    // The control / user-visible plane is up. Tell the bootlog we're done
    // with the boot phase and paint the "services up" line.
    bootlog::handoff();

    // Log the IP address once netd drops its ready-file.
    if let Ok(contents) = std::fs::read_to_string(w767_paths::NETD_READY_FILE) {
        for line in contents.lines().take(4) {
            console_log(&format!("[ INFO ] netd: {line}"));
        }
    } else if Path::new(w767_paths::NETD_READY_FILE).exists() {
        console_log("[ WARN ] netd.ready present but unreadable");
    }

    // Spawn supervisors.
    spawn_supervisors(initial);

    // Reap adopted zombies forever.
    reap_forever();
}
