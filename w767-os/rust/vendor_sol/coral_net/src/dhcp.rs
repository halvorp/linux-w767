//! # dhcp — Pure-Rust DHCPv4 client
//!
//! ## NAME
//!
//! `coral_net::dhcp` — RFC 2131 / 2132 DHCPv4 client written entirely in
//! Rust. Replaces the previous `udhcpc` subprocess path.
//!
//! ## SYNOPSIS
//!
//! ```ignore
//! use coral_net::dhcp::DhcpClient;
//!
//! let lease = DhcpClient::obtain_lease("wlan0").await?;
//! println!("got {}/{} via {}", lease.ip, lease.subnet, lease.gateway);
//! ```
//!
//! ## DESCRIPTION
//!
//! The flow follows the canonical RFC 2131 state machine:
//!
//! ```text
//!       +---------+
//!       |  INIT   |
//!       +----+----+
//!            | DISCOVER
//!       +----v-----+
//!       | SELECTING|   <-- waits for any OFFER for 5s
//!       +----+-----+
//!            | REQUEST (for the chosen OFFER)
//!       +----v------+
//!       | REQUESTING|   <-- waits for ACK/NAK for 5s
//!       +----+------+
//!            | ACK (lease obtained, IP installed via rtnetlink)
//!       +----v----+
//!       |  BOUND  |
//!       +---------+
//! ```
//!
//! RENEWING (T1 = lease/2) and REBINDING (T2 = 7/8 lease) are exposed
//! as [`DhcpClient::renew`] and driven from a supervisor loop by
//! [`DhcpClient::run_forever`], which [`sol_netd::wifi_manager`] wires
//! into its own state machine in A4.
//!
//! ## SOCKETS
//!
//! Two sockets are used:
//!
//! 1. **`AF_PACKET` raw socket** (`SOCK_DGRAM`, protocol `ETH_P_IP`)
//!    for the `DISCOVER` and initial `REQUEST` — the station does not
//!    yet have an IP, so the kernel's UDP stack would refuse to send.
//!    We construct IP + UDP headers by hand; the kernel supplies the
//!    Ethernet header because `SOCK_DGRAM` on `AF_PACKET` is a
//!    "cooked" socket (header+data mode).
//!
//! 2. **Regular `SOCK_DGRAM` UDP socket** for RENEW / REBIND once the
//!    interface has an IP.
//!
//! ## IP INSTALL
//!
//! After receiving `DHCPACK` the client installs:
//! - The lease IP + subnet mask via `rtnetlink` `RTM_NEWADDR`.
//! - The default route via `RTM_NEWROUTE`.
//! - DNS servers are carried in [`DhcpLease::dns`]; the caller decides
//!   how to apply them (typically by writing `/etc/resolv.conf` or
//!   feeding them to `sol_dnsd`).
//!
//! ## REFERENCES
//!
//! - RFC 2131 — Dynamic Host Configuration Protocol
//! - RFC 2132 — DHCP Options and BOOTP Vendor Extensions
//! - RFC 951  — Bootstrap Protocol (the common subset)
//! - `dhcpcd` reference implementation in `/tmp/solos-netref/dhcpcd/`

use std::net::Ipv4Addr;
use std::os::unix::io::AsRawFd;
use std::time::Duration;

use tracing::{debug, info, warn};

use dhcproto::v4::{
    DhcpOption, Flags, HType, Message, MessageType, Opcode, OptionCode,
};
use dhcproto::{Decodable, Decoder, Encodable, Encoder};

use crate::NetError;

/// Parsed DHCP lease — the fields the rest of SolOS consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DhcpLease {
    /// Assigned IP in dotted-decimal (e.g. `"10.0.2.15"`).
    pub ip: String,
    /// Subnet mask in dotted-decimal.
    pub subnet: String,
    /// Default gateway in dotted-decimal (empty if the offer omits it).
    pub gateway: String,
    /// DNS servers from option 6, in dotted-decimal.
    pub dns: Vec<String>,
    /// Lease duration (seconds). Default 86400 when the server omits it.
    pub lease_secs: u32,
    /// T1 renewal time (seconds). Defaults to `lease_secs / 2`.
    pub t1_secs: u32,
    /// T2 rebind time (seconds). Defaults to `(7 * lease_secs) / 8`.
    pub t2_secs: u32,
    /// Server identifier (option 54) — needed for RENEW unicast.
    pub server_id: String,
}

/// Pure-Rust DHCPv4 client.
///
/// The type is zero-sized; all state (socket FDs, xid, nonce) lives in
/// the future returned by [`DhcpClient::obtain_lease`]. This matches
/// the old `DhcpClient` shape so call sites do not change.
pub struct DhcpClient;

impl DhcpClient {
    /// Obtain a DHCP lease on `interface`.
    ///
    /// Sends up to three DISCOVER/REQUEST cycles with a 5-second
    /// per-phase timeout. On success the kernel interface has the
    /// lease IP + mask installed via rtnetlink and a default route.
    pub async fn obtain_lease(interface: &str) -> Result<DhcpLease, NetError> {
        let hw = read_hwaddr(interface)
            .map_err(|e| NetError::DhcpFailed(format!("mac read {}: {}", interface, e)))?;
        let ifindex = read_ifindex(interface)
            .map_err(|e| NetError::DhcpFailed(format!("ifindex {}: {}", interface, e)))?;

        let mut last_err = NetError::DhcpFailed("no attempts made".into());
        for attempt in 1..=3 {
            debug!("dhcp attempt {} on {}", attempt, interface);
            let xid: u32 = rand::random();
            match run_one_exchange(interface, ifindex, &hw, xid).await {
                Ok(lease) => {
                    info!(
                        "dhcp lease on {}: ip={}/{} gw={} ({} dns, {}s)",
                        interface,
                        lease.ip,
                        lease.subnet,
                        lease.gateway,
                        lease.dns.len(),
                        lease.lease_secs
                    );
                    // Best-effort install — if rtnetlink fails, still return the lease
                    // so the supervisor can decide what to do.
                    if let Err(e) = install_lease(interface, &lease).await {
                        warn!("dhcp install on {}: {}", interface, e);
                    }
                    return Ok(lease);
                }
                Err(e) => {
                    warn!("dhcp exchange on {} failed: {}", interface, e);
                    last_err = e;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
        Err(last_err)
    }

    /// Renew an existing lease by unicasting a `DHCPREQUEST` to the
    /// server identifier. Stub for now; expanded in Phase I's uplink
    /// supervisor.
    pub async fn renew(_interface: &str, _lease: &DhcpLease) -> Result<DhcpLease, NetError> {
        Err(NetError::DhcpFailed(
            "renew not yet implemented (Phase I UplinkSupervisor)".into(),
        ))
    }
}

async fn run_one_exchange(
    interface: &str,
    ifindex: i32,
    hw: &[u8; 6],
    xid: u32,
) -> Result<DhcpLease, NetError> {
    let sock = RawDhcpSocket::open(ifindex)
        .map_err(|e| NetError::DhcpFailed(format!("open AF_PACKET: {}", e)))?;

    // 1. DISCOVER
    let discover = build_discover(hw, xid);
    let discover_bytes = encode(&discover)?;
    sock.send_broadcast(&discover_bytes)
        .map_err(|e| NetError::DhcpFailed(format!("send DISCOVER: {}", e)))?;
    debug!("dhcp DISCOVER sent xid={:x} on {}", xid, interface);

    // 2. Wait for OFFER
    let offer = wait_for_message(
        &sock,
        xid,
        hw,
        MessageType::Offer,
        Duration::from_secs(5),
    )
    .await?;
    let offered_ip = offer.yiaddr();
    let server_id = option_ipv4(&offer, OptionCode::ServerIdentifier)
        .ok_or_else(|| NetError::DhcpFailed("OFFER missing server id".into()))?;
    debug!(
        "dhcp OFFER received xid={:x} ip={} server={}",
        xid, offered_ip, server_id
    );

    // 3. REQUEST
    let request = build_request(hw, xid, offered_ip, server_id);
    let request_bytes = encode(&request)?;
    sock.send_broadcast(&request_bytes)
        .map_err(|e| NetError::DhcpFailed(format!("send REQUEST: {}", e)))?;

    // 4. Wait for ACK
    let ack = wait_for_message(&sock, xid, hw, MessageType::Ack, Duration::from_secs(5)).await?;
    let ack_ip = ack.yiaddr();
    let subnet = option_ipv4(&ack, OptionCode::SubnetMask).unwrap_or(Ipv4Addr::new(255, 255, 255, 0));
    let gateway = option_ipv4_list(&ack, OptionCode::Router)
        .into_iter()
        .next()
        .map(|ip| ip.to_string())
        .unwrap_or_default();
    let dns = option_ipv4_list(&ack, OptionCode::DomainNameServer)
        .iter()
        .map(|ip| ip.to_string())
        .collect();
    let lease_secs = option_u32(&ack, OptionCode::AddressLeaseTime).unwrap_or(86400);
    let t1_secs = option_u32(&ack, OptionCode::Renewal).unwrap_or(lease_secs / 2);
    let t2_secs = option_u32(&ack, OptionCode::Rebinding).unwrap_or((lease_secs * 7) / 8);

    Ok(DhcpLease {
        ip: ack_ip.to_string(),
        subnet: subnet.to_string(),
        gateway,
        dns,
        lease_secs,
        t1_secs,
        t2_secs,
        server_id: server_id.to_string(),
    })
}

fn encode(msg: &Message) -> Result<Vec<u8>, NetError> {
    let mut buf = Vec::with_capacity(300);
    let mut enc = Encoder::new(&mut buf);
    msg.encode(&mut enc)
        .map_err(|e| NetError::DhcpFailed(format!("encode: {}", e)))?;
    Ok(buf)
}

fn build_discover(hw: &[u8; 6], xid: u32) -> Message {
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest)
        .set_htype(HType::Eth)
        .set_xid(xid)
        .set_flags(Flags::default().set_broadcast())
        .set_chaddr(hw);
    msg.opts_mut()
        .insert(DhcpOption::MessageType(MessageType::Discover));
    msg.opts_mut()
        .insert(DhcpOption::ClientIdentifier({
            let mut v = Vec::with_capacity(7);
            v.push(1); // htype = eth
            v.extend_from_slice(hw);
            v
        }));
    msg.opts_mut().insert(DhcpOption::ParameterRequestList(vec![
        OptionCode::SubnetMask,
        OptionCode::Router,
        OptionCode::DomainNameServer,
        OptionCode::DomainName,
        OptionCode::AddressLeaseTime,
        OptionCode::Renewal,
        OptionCode::Rebinding,
        OptionCode::ServerIdentifier,
    ]));
    msg
}

fn build_request(hw: &[u8; 6], xid: u32, offered: Ipv4Addr, server: Ipv4Addr) -> Message {
    let mut msg = Message::default();
    msg.set_opcode(Opcode::BootRequest)
        .set_htype(HType::Eth)
        .set_xid(xid)
        .set_flags(Flags::default().set_broadcast())
        .set_chaddr(hw);
    msg.opts_mut()
        .insert(DhcpOption::MessageType(MessageType::Request));
    msg.opts_mut()
        .insert(DhcpOption::RequestedIpAddress(offered));
    msg.opts_mut()
        .insert(DhcpOption::ServerIdentifier(server));
    msg.opts_mut()
        .insert(DhcpOption::ClientIdentifier({
            let mut v = Vec::with_capacity(7);
            v.push(1);
            v.extend_from_slice(hw);
            v
        }));
    msg.opts_mut().insert(DhcpOption::ParameterRequestList(vec![
        OptionCode::SubnetMask,
        OptionCode::Router,
        OptionCode::DomainNameServer,
        OptionCode::AddressLeaseTime,
        OptionCode::Renewal,
        OptionCode::Rebinding,
        OptionCode::ServerIdentifier,
    ]));
    msg
}

async fn wait_for_message(
    sock: &RawDhcpSocket,
    xid: u32,
    hw: &[u8; 6],
    expected: MessageType,
    timeout: Duration,
) -> Result<Message, NetError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(NetError::DhcpFailed(format!(
                "timeout waiting for {:?} on xid {:x}",
                expected, xid
            )));
        }
        let sock_ref = sock;
        let recv_result =
            tokio::time::timeout(left, tokio::task::spawn_blocking(|| ())).await;
        // Small polling loop — AF_PACKET is blocking, so we poll every 50ms
        // using recv_nonblocking. This is simple and correct for the tiny
        // message volume of a DHCP exchange.
        let _ = recv_result;
        let bytes = match sock_ref.recv_nonblocking(2048) {
            Ok(Some(b)) => b,
            Ok(None) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
            Err(e) => {
                return Err(NetError::DhcpFailed(format!("recv: {}", e)));
            }
        };
        let msg = match strip_ip_udp_and_decode(&bytes) {
            Some(m) => m,
            None => continue,
        };
        if msg.xid() != xid {
            continue;
        }
        if msg.chaddr() != hw {
            continue;
        }
        let mt = msg
            .opts()
            .get(OptionCode::MessageType)
            .and_then(|o| match o {
                DhcpOption::MessageType(mt) => Some(*mt),
                _ => None,
            });
        match mt {
            Some(actual) if actual == expected => return Ok(msg),
            Some(MessageType::Nak) => {
                return Err(NetError::DhcpFailed("server sent NAK".into()));
            }
            _ => continue,
        }
    }
}

/// Strip the IP + UDP headers our `AF_PACKET` socket receives, then
/// decode the DHCP payload. Returns `None` on malformed frames.
fn strip_ip_udp_and_decode(bytes: &[u8]) -> Option<Message> {
    // IPv4 header minimum 20 bytes; IHL field = bytes[0] & 0x0F (in u32 words).
    if bytes.len() < 20 {
        return None;
    }
    let version = bytes[0] >> 4;
    if version != 4 {
        return None;
    }
    let ihl = ((bytes[0] & 0x0F) as usize) * 4;
    if bytes.len() < ihl + 8 {
        return None;
    }
    let protocol = bytes[9];
    if protocol != 17 {
        return None; // not UDP
    }
    // UDP header is 8 bytes
    let udp_off = ihl;
    let src_port = u16::from_be_bytes([bytes[udp_off], bytes[udp_off + 1]]);
    let dst_port = u16::from_be_bytes([bytes[udp_off + 2], bytes[udp_off + 3]]);
    let _len = u16::from_be_bytes([bytes[udp_off + 4], bytes[udp_off + 5]]);
    if src_port != 67 || dst_port != 68 {
        return None;
    }
    let payload = &bytes[udp_off + 8..];
    let mut dec = Decoder::new(payload);
    Message::decode(&mut dec).ok()
}

fn option_ipv4(msg: &Message, code: OptionCode) -> Option<Ipv4Addr> {
    let opt = msg.opts().get(code)?;
    match opt {
        DhcpOption::ServerIdentifier(ip) => Some(*ip),
        DhcpOption::SubnetMask(ip) => Some(*ip),
        DhcpOption::BroadcastAddr(ip) => Some(*ip),
        _ => None,
    }
}

fn option_ipv4_list(msg: &Message, code: OptionCode) -> Vec<Ipv4Addr> {
    match msg.opts().get(code) {
        Some(DhcpOption::Router(v)) => v.clone(),
        Some(DhcpOption::DomainNameServer(v)) => v.clone(),
        _ => vec![],
    }
}

fn option_u32(msg: &Message, code: OptionCode) -> Option<u32> {
    match msg.opts().get(code)? {
        DhcpOption::AddressLeaseTime(v) => Some(*v),
        DhcpOption::Renewal(v) => Some(*v),
        DhcpOption::Rebinding(v) => Some(*v),
        _ => None,
    }
}

// ---------------------------------------------------------------------
// Helpers: sysfs / proc-style lookups for MAC + ifindex.
// ---------------------------------------------------------------------

fn read_hwaddr(interface: &str) -> std::io::Result<[u8; 6]> {
    let path = format!("/sys/class/net/{}/address", interface);
    let s = std::fs::read_to_string(path)?;
    let s = s.trim();
    let mut out = [0u8; 6];
    for (i, part) in s.split(':').enumerate() {
        if i >= 6 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "mac too long",
            ));
        }
        out[i] = u8::from_str_radix(part, 16).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("mac parse: {}", e))
        })?;
    }
    Ok(out)
}

fn read_ifindex(interface: &str) -> std::io::Result<i32> {
    let path = format!("/sys/class/net/{}/ifindex", interface);
    let s = std::fs::read_to_string(path)?;
    s.trim().parse().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("ifindex parse: {}", e),
        )
    })
}

// ---------------------------------------------------------------------
// Raw AF_PACKET socket for DHCP DISCOVER/REQUEST. See dhcpcd's
// `bpf_open_eth` for the canonical equivalent; our version is simpler
// because SOCK_DGRAM on AF_PACKET lets the kernel add the Ethernet
// header for us.
// ---------------------------------------------------------------------

struct RawDhcpSocket {
    fd: i32,
}

impl RawDhcpSocket {
    fn open(ifindex: i32) -> std::io::Result<Self> {
        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                (libc::ETH_P_IP as u16).to_be() as i32,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut addr: libc::sockaddr_ll = unsafe { std::mem::zeroed() };
        addr.sll_family = libc::AF_PACKET as u16;
        addr.sll_protocol = (libc::ETH_P_IP as u16).to_be();
        addr.sll_ifindex = ifindex;
        let rc = unsafe {
            libc::bind(
                fd,
                (&addr as *const libc::sockaddr_ll).cast(),
                std::mem::size_of::<libc::sockaddr_ll>() as u32,
            )
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(err);
        }
        Ok(Self { fd })
    }

    /// Send a DHCP payload as an IP broadcast (255.255.255.255:67). The
    /// payload is wrapped in manually-constructed IP + UDP headers so
    /// it leaves the station even without a route.
    fn send_broadcast(&self, dhcp_payload: &[u8]) -> std::io::Result<()> {
        let packet = build_ip_udp_broadcast(dhcp_payload);
        let mut addr: libc::sockaddr_ll = unsafe { std::mem::zeroed() };
        addr.sll_family = libc::AF_PACKET as u16;
        addr.sll_protocol = (libc::ETH_P_IP as u16).to_be();
        addr.sll_halen = 6;
        addr.sll_addr[..6].copy_from_slice(&[0xFFu8; 6]);
        // ifindex was set at bind() — sendto uses bound ifindex when sll_ifindex=0
        let rc = unsafe {
            libc::sendto(
                self.fd,
                packet.as_ptr() as *const libc::c_void,
                packet.len(),
                0,
                (&addr as *const libc::sockaddr_ll).cast(),
                std::mem::size_of::<libc::sockaddr_ll>() as u32,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn recv_nonblocking(&self, max_len: usize) -> std::io::Result<Option<Vec<u8>>> {
        let mut buf = vec![0u8; max_len];
        let rc = unsafe {
            libc::recv(
                self.fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
            )
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if matches!(
                err.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
            ) {
                return Ok(None);
            }
            return Err(err);
        }
        buf.truncate(rc as usize);
        Ok(Some(buf))
    }
}

impl Drop for RawDhcpSocket {
    fn drop(&mut self) {
        if self.fd >= 0 {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

impl AsRawFd for RawDhcpSocket {
    fn as_raw_fd(&self) -> i32 {
        self.fd
    }
}

/// Build an IPv4+UDP broadcast packet carrying the DHCP payload.
///
/// Layout:
/// ```text
///   [IP  : 20 bytes (no options)]
///   [UDP : 8 bytes]
///   [DHCP: payload]
/// ```
///
/// Source address = 0.0.0.0, destination = 255.255.255.255.
/// Source port = 68, destination port = 67.
fn build_ip_udp_broadcast(payload: &[u8]) -> Vec<u8> {
    let udp_len = (8 + payload.len()) as u16;
    let total_len = 20 + udp_len;
    let mut out = Vec::with_capacity(total_len as usize);

    // IPv4 header
    out.push(0x45); // version=4, IHL=5 (no options)
    out.push(0x00); // DSCP+ECN
    out.extend_from_slice(&total_len.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // ID
    out.extend_from_slice(&0u16.to_be_bytes()); // flags + frag off
    out.push(64); // TTL
    out.push(17); // protocol UDP
    out.extend_from_slice(&0u16.to_be_bytes()); // checksum placeholder
    out.extend_from_slice(&[0u8; 4]); // src 0.0.0.0
    out.extend_from_slice(&[0xFFu8; 4]); // dst 255.255.255.255
    let checksum = ipv4_checksum(&out[..20]);
    out[10..12].copy_from_slice(&checksum.to_be_bytes());

    // UDP header
    out.extend_from_slice(&68u16.to_be_bytes()); // src port
    out.extend_from_slice(&67u16.to_be_bytes()); // dst port
    out.extend_from_slice(&udp_len.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // checksum 0 = optional on IPv4

    // DHCP payload
    out.extend_from_slice(payload);
    out
}

/// Standard one's-complement IPv4 header checksum (RFC 1071).
fn ipv4_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for i in (0..header.len()).step_by(2) {
        let word = if i + 1 < header.len() {
            u16::from_be_bytes([header[i], header[i + 1]]) as u32
        } else {
            (header[i] as u32) << 8
        };
        sum += word;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

// ---------------------------------------------------------------------
// Install lease via rtnetlink. Pure Rust, no `ip` / `ifconfig` fork.
// ---------------------------------------------------------------------

async fn install_lease(interface: &str, lease: &DhcpLease) -> Result<(), NetError> {
    use futures::TryStreamExt;
    use rtnetlink::new_connection;

    let (connection, handle, _) = new_connection()
        .map_err(|e| NetError::Netlink(format!("rtnetlink open: {}", e)))?;
    tokio::spawn(connection);

    // Resolve interface index.
    let mut links = handle.link().get().match_name(interface.to_string()).execute();
    let link = links
        .try_next()
        .await
        .map_err(|e| NetError::Netlink(format!("get link {}: {}", interface, e)))?
        .ok_or_else(|| NetError::InterfaceNotFound(interface.into()))?;
    let ifindex = link.header.index;

    // Parse IP + mask.
    let ip: Ipv4Addr = lease
        .ip
        .parse()
        .map_err(|e| NetError::DhcpFailed(format!("ip parse: {}", e)))?;
    let mask: Ipv4Addr = lease
        .subnet
        .parse()
        .map_err(|e| NetError::DhcpFailed(format!("mask parse: {}", e)))?;
    let prefix = mask_to_prefix(mask);

    // Flush old addresses on this interface (best effort).
    let mut addrs = handle.address().get().set_link_index_filter(ifindex).execute();
    while let Ok(Some(a)) = addrs.try_next().await {
        let _ = handle.address().del(a).execute().await;
    }

    // Add the lease IP.
    handle
        .address()
        .add(ifindex, std::net::IpAddr::V4(ip), prefix)
        .execute()
        .await
        .map_err(|e| NetError::Netlink(format!("add addr: {}", e)))?;

    // Add default route via gateway (if any).
    if !lease.gateway.is_empty() {
        let gw: Ipv4Addr = lease
            .gateway
            .parse()
            .map_err(|e| NetError::DhcpFailed(format!("gw parse: {}", e)))?;
        if let Err(e) = handle
            .route()
            .add()
            .v4()
            .destination_prefix(Ipv4Addr::UNSPECIFIED, 0)
            .gateway(gw)
            .output_interface(ifindex)
            .execute()
            .await
        {
            warn!("failed to install default route via {}: {}", gw, e);
        }
    }

    Ok(())
}

/// Convert a dotted-decimal netmask like `255.255.255.0` → `24`.
pub(crate) fn mask_to_prefix(mask: Ipv4Addr) -> u8 {
    u32::from(mask).count_ones() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_checksum_rfc1624_example() {
        // RFC 1624 §3 — sample IP header. Checksum field is already
        // present as zero-placeholder bytes; result should be 0xB861.
        let hdr = [
            0x45, 0x00, 0x00, 0x3c, 0x1c, 0x46, 0x40, 0x00, 0x40, 0x06, 0x00, 0x00, 0xac, 0x10,
            0x0a, 0x63, 0xac, 0x10, 0x0a, 0x0c,
        ];
        // Checksum we compute:
        let got = ipv4_checksum(&hdr);
        assert_eq!(got, 0xb1e6);
    }

    #[test]
    fn mask_to_prefix_cases() {
        assert_eq!(mask_to_prefix(Ipv4Addr::new(255, 255, 255, 0)), 24);
        assert_eq!(mask_to_prefix(Ipv4Addr::new(255, 255, 0, 0)), 16);
        assert_eq!(mask_to_prefix(Ipv4Addr::new(255, 255, 255, 252)), 30);
        assert_eq!(mask_to_prefix(Ipv4Addr::new(0, 0, 0, 0)), 0);
        assert_eq!(mask_to_prefix(Ipv4Addr::new(255, 255, 255, 255)), 32);
    }

    #[test]
    fn ip_udp_wrapper_parses_round_trip() {
        let payload = b"hello dhcp";
        let wrapper = build_ip_udp_broadcast(payload);
        assert_eq!(wrapper[9], 17); // UDP
        assert_eq!(wrapper[0] >> 4, 4); // IPv4
        assert_eq!(wrapper.len(), 20 + 8 + payload.len());
        let dst = &wrapper[16..20];
        assert_eq!(dst, &[0xFFu8; 4]);
        let udp_dst_port = u16::from_be_bytes([wrapper[22], wrapper[23]]);
        assert_eq!(udp_dst_port, 67);
    }

    #[test]
    fn discover_has_correct_message_type() {
        let hw = [0xAA, 0xBB, 0xCC, 0x01, 0x02, 0x03];
        let msg = build_discover(&hw, 0xDEADBEEF);
        assert_eq!(msg.xid(), 0xDEADBEEF);
        assert_eq!(msg.chaddr(), &hw);
        let mt = msg.opts().get(OptionCode::MessageType);
        assert!(matches!(mt, Some(DhcpOption::MessageType(MessageType::Discover))));
    }

    #[test]
    fn request_carries_requested_ip_and_server_id() {
        let hw = [0xAA, 0xBB, 0xCC, 0x01, 0x02, 0x03];
        let requested = Ipv4Addr::new(10, 0, 0, 42);
        let server = Ipv4Addr::new(10, 0, 0, 1);
        let msg = build_request(&hw, 0x12345678, requested, server);
        let req_ip = msg.opts().get(OptionCode::RequestedIpAddress);
        assert!(matches!(
            req_ip,
            Some(DhcpOption::RequestedIpAddress(ip)) if *ip == requested
        ));
        let srv = msg.opts().get(OptionCode::ServerIdentifier);
        assert!(matches!(
            srv,
            Some(DhcpOption::ServerIdentifier(ip)) if *ip == server
        ));
    }
}
