//! w767_ctl server — UDS-only tarpc daemon for driver-dev control.
//!
//! Transport: `/run/w767/ctl.sock`, gated by SO_PEERCRED (root only).
//! A TCP frontend is intentionally absent at MVP — use SSH (dropbear) + the
//! bundled `w767_ctl_cli` to reach this socket. If remote TCP is later needed,
//! add a thin token-gated wrapper in front of the UDS path.

use std::process::Stdio;
use std::time::Duration;

use futures::StreamExt;
use tarpc::server::{BaseChannel, Channel};
use tokio::process::Command;
use tracing::{info, warn};

use sol_ipc::notify_ready;
use sol_ipc::transport::{bind_tarpc_socket, get_peer_uid, make_tarpc_transport};
use w767_ctl::{
    IfaceSummary, ModuleEntry, RebootKind, RunResult, StatusReport,
    W767Ctl,
};

const BUILD_VERSION: &str = concat!("w767_ctl ", env!("CARGO_PKG_VERSION"));

// ---------------------------------------------------------------------------
// Service implementation
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CtlSvc;

impl W767Ctl for CtlSvc {
    async fn version(self, _ctx: tarpc::context::Context) -> String {
        BUILD_VERSION.to_string()
    }

    async fn status(self, _ctx: tarpc::context::Context) -> StatusReport {
        let uptime_secs = parse_uptime();
        let loadavg = parse_loadavg();
        let (mem_total_kb, mem_free_kb) = parse_meminfo();
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .unwrap_or_default()
            .trim()
            .to_string();
        let iface_summary = summarize_interfaces();
        StatusReport {
            version: BUILD_VERSION.to_string(),
            kernel,
            uptime_secs,
            loadavg,
            mem_total_kb,
            mem_free_kb,
            iface_summary,
        }
    }

    async fn lsmod(self, _ctx: tarpc::context::Context) -> Vec<ModuleEntry> {
        parse_modules()
    }

    async fn modprobe(self, _ctx: tarpc::context::Context, name: String) -> Result<String, String> {
        validate_module_name(&name)?;
        let out = Command::new("/sbin/modprobe")
            .arg(&name)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("spawn modprobe: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(format!(
                "modprobe {name} exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    async fn rmmod(self, _ctx: tarpc::context::Context, name: String, force: bool) -> Result<String, String> {
        validate_module_name(&name)?;
        let mut args: Vec<&str> = Vec::new();
        if force { args.push("--force"); }
        args.push(&name);
        let out = Command::new("/sbin/rmmod")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("spawn rmmod: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(format!(
                "rmmod {name} exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    async fn dmesg_tail(self, _ctx: tarpc::context::Context, n: usize) -> Vec<String> {
        // klogctl SYSLOG_ACTION_READ_ALL = 3; buffer is kernel-side, ~256 KB.
        let mut buf = vec![0u8; 256 * 1024];
        // SAFETY: klogctl reads into our buffer; buf.len() is the capacity.
        let n_read = unsafe {
            libc::klogctl(3, buf.as_mut_ptr() as *mut _, buf.len() as i32)
        };
        if n_read < 0 { return Vec::new(); }
        buf.truncate(n_read as usize);
        let text = String::from_utf8_lossy(&buf);
        let lines: Vec<&str> = text.lines().collect();
        let start = lines.len().saturating_sub(n.max(1));
        lines[start..].iter().map(|s| s.to_string()).collect()
    }

    async fn read_file(self, _ctx: tarpc::context::Context, path: String) -> Result<String, String> {
        tokio::fs::read_to_string(&path).await.map_err(|e| e.to_string())
    }

    async fn write_file(self, _ctx: tarpc::context::Context, path: String, content: String) -> Result<(), String> {
        tokio::fs::write(&path, content.as_bytes()).await.map_err(|e| e.to_string())
    }

    async fn run(self, _ctx: tarpc::context::Context, argv: Vec<String>, timeout_secs: u32) -> RunResult {
        if argv.is_empty() {
            return RunResult { exit_code: 127, stdout: String::new(),
                stderr: "empty argv".into(), timed_out: false };
        }
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return RunResult { exit_code: 127, stdout: String::new(),
                stderr: format!("spawn: {e}"), timed_out: false },
        };

        let fut = child.wait_with_output();
        let out = if timeout_secs == 0 {
            fut.await
        } else {
            match tokio::time::timeout(Duration::from_secs(timeout_secs as u64), fut).await {
                Ok(r) => r,
                Err(_) => return RunResult {
                    exit_code: -1, stdout: String::new(),
                    stderr: format!("timed out after {timeout_secs}s"),
                    timed_out: true,
                },
            }
        };
        match out {
            Ok(o) => RunResult {
                exit_code: o.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                timed_out: false,
            },
            Err(e) => RunResult {
                exit_code: -1, stdout: String::new(),
                stderr: format!("wait: {e}"), timed_out: false,
            },
        }
    }

    async fn reboot(self, _ctx: tarpc::context::Context, kind: RebootKind) -> () {
        // Sync the disks and spawn a detached task so the RPC returns cleanly.
        let _ = Command::new("/bin/sync").status().await;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            // SAFETY: libc::reboot is a straight syscall wrapper.
            let rc = match kind {
                RebootKind::Warm  => unsafe { libc::reboot(libc::RB_AUTOBOOT) },
                RebootKind::Cold  => unsafe { libc::reboot(libc::RB_POWER_OFF) },
                RebootKind::Kexec => unsafe { libc::reboot(libc::RB_KEXEC) },
            };
            warn!("reboot({:?}) returned {} (should never return)", kind, rc);
        });
    }

    async fn pstore_list(self, _ctx: tarpc::context::Context) -> Vec<String> {
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir("/sys/fs/pstore") {
            for e in rd.flatten() {
                if let Some(s) = e.file_name().to_str() {
                    entries.push(s.to_string());
                }
            }
        }
        entries.sort();
        entries
    }

    async fn pstore_read(self, _ctx: tarpc::context::Context, name: String) -> Result<String, String> {
        if name.contains('/') || name.contains("..") {
            return Err("invalid pstore entry name".into());
        }
        let path = format!("/sys/fs/pstore/{name}");
        tokio::fs::read_to_string(&path).await.map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_uptime() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(|s| s.to_string()))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as u64)
        .unwrap_or(0)
}

fn parse_loadavg() -> [f32; 3] {
    let mut out = [0.0; 3];
    if let Ok(s) = std::fs::read_to_string("/proc/loadavg") {
        for (i, tok) in s.split_whitespace().take(3).enumerate() {
            out[i] = tok.parse().unwrap_or(0.0);
        }
    }
    out
}

fn parse_meminfo() -> (u64, u64) {
    let mut total = 0u64; let mut free = 0u64;
    if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
        for line in s.lines() {
            let mut it = line.split_whitespace();
            match (it.next(), it.next()) {
                (Some("MemTotal:"), Some(v)) => total = v.parse().unwrap_or(0),
                (Some("MemAvailable:"), Some(v)) => free = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    (total, free)
}

fn parse_modules() -> Vec<ModuleEntry> {
    let mut out = Vec::new();
    let Ok(s) = std::fs::read_to_string("/proc/modules") else { return out; };
    for line in s.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 { continue; }
        out.push(ModuleEntry {
            name: cols[0].to_string(),
            size: cols[1].parse().unwrap_or(0),
            refcount: cols[2].parse().unwrap_or(0),
            used_by: if cols[3] == "-" { Vec::new() }
                     else { cols[3].split(',').filter(|s| !s.is_empty()).map(str::to_string).collect() },
        });
    }
    out
}

fn summarize_interfaces() -> Vec<IfaceSummary> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/class/net") else { return out; };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let is_up = std::fs::read_to_string(format!("/sys/class/net/{name}/operstate"))
            .map(|s| s.trim() == "up").unwrap_or(false);
        let has_carrier = std::fs::read_to_string(format!("/sys/class/net/{name}/carrier"))
            .map(|s| s.trim() == "1").unwrap_or(false);
        let addrs = read_iface_addrs(&name);
        out.push(IfaceSummary { name, is_up, has_carrier, addrs });
    }
    out
}

fn read_iface_addrs(iface: &str) -> Vec<String> {
    // We can't easily get IPs out of sysfs. Shell out to ip(8) via busybox
    // and parse; tiny, adequate for a status report.
    use std::process::Command;
    let out = Command::new("/opt/w767/bin/busybox")
        .args(["ip", "-o", "addr", "show", iface])
        .output();
    let Ok(out) = out else { return Vec::new(); };
    if !out.status.success() { return Vec::new(); }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            while let Some(tok) = it.next() {
                if tok == "inet" || tok == "inet6" {
                    return it.next().map(str::to_string);
                }
            }
            None
        })
        .collect()
}

fn validate_module_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err("invalid module name length".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err("module name contains forbidden characters".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn setup_tracing() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    setup_tracing();
    info!("{BUILD_VERSION} starting");

    let listener = bind_tarpc_socket(sol_ipc::SOCK_CTL)?;
    info!("w767_ctl listening on {}", sol_ipc::socket_path(sol_ipc::SOCK_CTL));
    notify_ready();

    use std::os::unix::io::AsRawFd;
    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => { warn!("accept: {e}"); continue; }
        };

        // SO_PEERCRED root gate. Any other UID is rejected.
        let peer = get_peer_uid(stream.as_raw_fd());
        if peer != Some(0) {
            warn!("rejecting peer uid={:?}", peer);
            drop(stream);
            continue;
        }

        let transport = make_tarpc_transport(stream);
        let channel = BaseChannel::with_defaults(transport);
        tokio::spawn(
            channel
                .execute(CtlSvc.serve())
                .for_each(|resp| async { tokio::spawn(resp); }),
        );
    }
}
