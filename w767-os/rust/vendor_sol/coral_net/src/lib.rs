//! Trimmed vendor of `sol-os::coral_net`.
//!
//! Only the leaf modules used by w767_netd_lite are vendored:
//! - [`interface`] — pure-Rust rtnetlink wrapper (link up/down, addr enum).
//! - [`dhcp`]      — pure-Rust RFC 2131 DHCPv4 client.
//!
//! Everything else (wpa, dns, wireguard, netns, zones, site-policy, routing,
//! socks, nft_rules, fib_rules, eapol_crypto) is dropped. If/when we need
//! those, re-vendor them under this same crate rather than forking coral_net.

pub mod dhcp;
pub mod interface;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("interface not found: {0}")]
    InterfaceNotFound(String),
    #[error("netlink error: {0}")]
    Netlink(String),
    #[error("dhcp failed: {0}")]
    DhcpFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
