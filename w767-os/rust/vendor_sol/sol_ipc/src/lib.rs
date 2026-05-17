//! Trimmed vendor of `sol-os::sol_ipc`.
//!
//! Only the UDS transport primitives are kept:
//! - `PeerUid`, `get_peer_uid` for SO_PEERCRED-based auth on Unix sockets.
//! - `bind_tarpc_socket`, `make_tarpc_transport`, `connect_tarpc_raw` for
//!   tarpc-over-UDS transport.
//! - `notify_ready()` — writes `READY\n` to the `W767_NOTIFY_FD` pipe that
//!   `w767_init` creates for each supervised daemon.
//!
//! The rest of sol_ipc (action, auth, cursor_shm, fast_input, fd_pass,
//! settings, services, subscription, types) is dropped. If a real service
//! trait is needed, define it in the consuming crate with `#[tarpc::service]`.

pub mod transport;

// ---------------------------------------------------------------------------
// Socket paths
// ---------------------------------------------------------------------------

/// IPC socket directory. Production: `/run/w767/` (see w767_paths::W767_RUN).
/// Dev fallback (host): `/tmp/w767`.
pub fn socket_dir() -> &'static str {
    if std::path::Path::new(w767_paths::W767_BIN).exists() {
        w767_paths::W767_RUN
    } else {
        "/tmp/w767"
    }
}

/// Full path to a named socket, e.g. `/run/w767/ctl.sock`.
pub fn socket_path(name: &str) -> String {
    format!("{}/{}", socket_dir(), name)
}

/// Socket name for the control daemon.
pub const SOCK_CTL: &str = "ctl.sock";
/// Socket name for the network daemon.
pub const SOCK_NETD: &str = "netd.sock";

// ---------------------------------------------------------------------------
// RPC context helpers
// ---------------------------------------------------------------------------

pub fn rpc_context() -> tarpc::context::Context {
    let mut ctx = tarpc::context::current();
    ctx.deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    ctx
}

pub fn rpc_context_secs(secs: u64) -> tarpc::context::Context {
    let mut ctx = tarpc::context::current();
    ctx.deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    ctx
}

// ---------------------------------------------------------------------------
// Daemon readiness signaling
// ---------------------------------------------------------------------------

/// Name of the env var carrying the readiness-pipe write-end FD.
pub const NOTIFY_FD_ENV: &str = "W767_NOTIFY_FD";

/// Signal readiness to `w767_init` via the `W767_NOTIFY_FD` pipe.
///
/// `w767_init` creates a pipe for each supervised daemon and passes the write
/// end as `W767_NOTIFY_FD`. Daemons call this after binding their socket to
/// signal they're ready to accept connections. If the env var is absent
/// (dev mode / ran by hand), this is a silent no-op.
pub fn notify_ready() {
    let fd_str = match std::env::var(NOTIFY_FD_ENV) {
        Ok(s) => s,
        Err(_) => return,
    };
    let fd = match fd_str.parse::<i32>() {
        Ok(fd) if fd >= 0 => fd,
        _ => return,
    };
    // SAFETY: fd comes from w767_init's pipe; the 6-byte write is atomic on pipes,
    //         and write()/close() are async-signal-safe.
    unsafe {
        libc::write(fd, b"READY\n".as_ptr() as *const libc::c_void, 6);
        libc::close(fd);
    }
}
