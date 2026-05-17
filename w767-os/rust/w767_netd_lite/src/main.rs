//! w767_netd_lite — minimal network bring-up daemon.
//!
//! Flow:
//!   1. bring up `lo` (we'd like 127.0.0.1 for the UDS control socket path)
//!   2. enumerate interfaces via `InterfaceManager`
//!   3. pick the first non-loopback non-wireless interface with carrier
//!      (USB-Ethernet in Phase 2; later WCN3990 Wi-Fi once ath10k is up)
//!   4. run DHCP via `DhcpClient::obtain_lease` (which installs IP + route)
//!   5. write /run/w767/netd.ready with the lease
//!   6. signal `W767_NOTIFY_FD` READY
//!   7. idle (lease renewal = TODO; Phase 2 gets a long lease)

use std::time::Duration;

use coral_net::dhcp::{DhcpClient, DhcpLease};
use coral_net::interface::InterfaceManager;
use sol_ipc::notify_ready;
use tracing::{info, warn};
use w767_paths::NETD_READY_FILE;

fn setup_tracing() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

/// Pick the first non-lo, non-wireless interface that has carrier.
/// Wait up to `max_wait` for one to appear (USB-Ethernet enumeration is async).
async fn wait_for_ethernet(
    mgr: &InterfaceManager,
    max_wait: Duration,
) -> Option<String> {
    let deadline = std::time::Instant::now() + max_wait;
    loop {
        match mgr.list_interfaces().await {
            Ok(ifaces) => {
                for i in ifaces {
                    if i.name == "lo" || i.is_wireless {
                        continue;
                    }
                    // Check sysfs carrier; `i.is_up` means admin up, not link.
                    let carrier_path = format!("/sys/class/net/{}/carrier", i.name);
                    let has_carrier = std::fs::read_to_string(&carrier_path)
                        .map(|s| s.trim() == "1")
                        .unwrap_or(false);
                    if has_carrier {
                        info!("selected interface: {}", i.name);
                        return Some(i.name);
                    }
                }
            }
            Err(e) => warn!("list_interfaces: {}", e),
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn write_ready_file(iface: &str, lease: &DhcpLease) {
    let _ = std::fs::create_dir_all(w767_paths::W767_RUN);
    let contents = format!(
        "iface={iface}\nip={ip}\nsubnet={subnet}\ngateway={gw}\ndns={dns}\nlease={lease}\n",
        ip = lease.ip,
        subnet = lease.subnet,
        gw = lease.gateway,
        dns = lease.dns.join(","),
        lease = lease.lease_secs,
    );
    if let Err(e) = std::fs::write(NETD_READY_FILE, contents) {
        warn!("failed to write {}: {}", NETD_READY_FILE, e);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    setup_tracing();
    info!("w767_netd_lite starting");

    let mgr = InterfaceManager::new()
        .await
        .map_err(|e| anyhow::anyhow!("rtnetlink init failed: {}", e))?;

    // Bring lo up — best-effort; the kernel may already have it up.
    if let Err(e) = mgr.set_link_up("lo").await {
        warn!("set lo up: {}", e);
    }

    // Wait up to 30 s for a USB-Ethernet dongle to appear + carrier.
    let iface = match wait_for_ethernet(&mgr, Duration::from_secs(30)).await {
        Some(n) => n,
        None => {
            warn!("no ethernet interface with carrier after 30 s");
            // Still notify ready so the DAG doesn't stall; the user can debug
            // via the serial/framebuffer path or plug in a dongle and restart.
            notify_ready();
            return idle_forever().await;
        }
    };

    // Bring the iface up before DHCP starts (DhcpClient does not do this).
    if let Err(e) = mgr.set_link_up(&iface).await {
        warn!("set {} up: {}", iface, e);
    }

    // DHCP. obtain_lease installs IP + default route via rtnetlink internally.
    let lease = match DhcpClient::obtain_lease(&iface).await {
        Ok(l) => l,
        Err(e) => {
            warn!("DHCP failed on {}: {}", iface, e);
            notify_ready();
            return idle_forever().await;
        }
    };

    info!(
        "DHCP lease: {} on {} via {} (lease={}s, {} DNS)",
        lease.ip, iface, lease.gateway, lease.lease_secs, lease.dns.len()
    );
    write_ready_file(&iface, &lease);
    notify_ready();

    idle_forever().await
}

async fn idle_forever() -> anyhow::Result<()> {
    // TODO: renewal loop. For bring-up we rely on the server's long default
    // lease + a reboot-level refresh.
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}
