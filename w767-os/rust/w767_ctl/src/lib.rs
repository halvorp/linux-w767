//! w767_ctl — control plane for the w767-os driver-dev platform.
//!
//! The library half defines the tarpc service trait used by both the daemon
//! (`src/main.rs`) and the CLI (`w767_ctl_cli/src/main.rs`). Keeping it in a
//! shared lib crate means the two binaries agree on every method signature
//! at compile time.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub version: String,
    pub kernel: String,
    pub uptime_secs: u64,
    pub loadavg: [f32; 3],
    pub mem_total_kb: u64,
    pub mem_free_kb: u64,
    pub iface_summary: Vec<IfaceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfaceSummary {
    pub name: String,
    pub is_up: bool,
    pub has_carrier: bool,
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleEntry {
    pub name: String,
    pub size: u64,
    pub refcount: u32,
    pub used_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RebootKind {
    Warm,     // reboot(LINUX_REBOOT_CMD_RESTART)
    Cold,     // reboot(LINUX_REBOOT_CMD_POWER_OFF) then user power-cycles
    Kexec,    // reboot(LINUX_REBOOT_CMD_KEXEC) — requires preloaded kexec image
}

#[tarpc::service]
pub trait W767Ctl {
    /// Build string + git sha, if embedded.
    async fn version() -> String;

    /// One-shot snapshot of machine health.
    async fn status() -> StatusReport;

    /// Loaded kernel modules (parsed from /proc/modules).
    async fn lsmod() -> Vec<ModuleEntry>;

    /// Load a module by name. Returns combined modprobe stderr on failure.
    async fn modprobe(name: String) -> Result<String, String>;

    /// Unload a module. `force=true` adds `--force` to rmmod.
    async fn rmmod(name: String, force: bool) -> Result<String, String>;

    /// Last `n` lines of the kernel log (klogctl SYSLOG_ACTION_READ_ALL,
    /// trimmed to tail).
    async fn dmesg_tail(n: usize) -> Vec<String>;

    /// Read a file (full contents as String).
    async fn read_file(path: String) -> Result<String, String>;

    /// Write a file. Caller must pass the full desired contents.
    async fn write_file(path: String, content: String) -> Result<(), String>;

    /// Run an arbitrary command. `timeout_secs = 0` disables the deadline.
    async fn run(argv: Vec<String>, timeout_secs: u32) -> RunResult;

    /// Reboot the machine. Returns immediately; the reboot fires shortly after.
    async fn reboot(kind: RebootKind) -> ();

    /// List /sys/fs/pstore entries (Oopses captured across last reboot).
    async fn pstore_list() -> Vec<String>;

    /// Read one pstore entry by filename.
    async fn pstore_read(name: String) -> Result<String, String>;
}
