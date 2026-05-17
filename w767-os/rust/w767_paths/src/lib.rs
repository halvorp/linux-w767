//! Filesystem path constants for w767-os.
//!
//! On-device layout (inside the Phase 2 initramfs):
//!   /opt/w767/bin/   — our binaries (w767_init, w767_ctl, w767_netd_lite)
//!   /etc/w767/       — static config (ctl.token, services.toml)
//!   /run/w767/       — runtime state (sockets, netd.ready)
//!   /lib/firmware/   — baked firmware blobs (from firmware-stage/)
//!   /lib/modules/    — kernel modules (from build-kernel.sh)
//!
//! Replaces sol_paths::SYSTEM_CORE / SYSTEM_PREFS / SYSTEM_RUN_SOLOS.

pub const W767_BIN: &str = "/opt/w767/bin";
pub const W767_ETC: &str = "/etc/w767";
pub const W767_RUN: &str = "/run/w767";
pub const W767_LIB_FIRMWARE: &str = "/lib/firmware";
pub const W767_LIB_MODULES: &str = "/lib/modules";
pub const W767_LOG: &str = "/var/log/w767";

/// Preshared token for `w767_ctl` TCP authentication. Baked into the
/// initramfs at build time and stored on the dev host for the CLI.
pub const CTL_TOKEN_FILE: &str = "/etc/w767/ctl.token";

/// File written by `w767_netd_lite` once DHCP succeeds; contains IP/DNS/gateway.
pub const NETD_READY_FILE: &str = "/run/w767/netd.ready";
