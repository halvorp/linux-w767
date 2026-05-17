use std::net::IpAddr;

use futures::TryStreamExt;
use rtnetlink::Handle;
use netlink_packet_route::link::{LinkAttribute, LinkFlag};
use netlink_packet_route::address::AddressAttribute;
use tracing::debug;

use crate::NetError;

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub index: u32,
    pub is_up: bool,
    pub is_wireless: bool,
    pub addresses: Vec<IpAddr>,
}

pub struct InterfaceManager {
    handle: Handle,
}

impl InterfaceManager {
    /// Create a new InterfaceManager. Spawns the rtnetlink connection task.
    pub async fn new() -> Result<Self, NetError> {
        let (connection, handle, _) = rtnetlink::new_connection()
            .map_err(|e| NetError::Netlink(e.to_string()))?;
        tokio::spawn(connection);
        Ok(Self { handle })
    }

    /// List all network interfaces.
    pub async fn list_interfaces(&self) -> Result<Vec<InterfaceInfo>, NetError> {
        let mut links = self.handle.link().get().execute();
        let mut interfaces = Vec::new();

        while let Some(msg) = links
            .try_next()
            .await
            .map_err(|e| NetError::Netlink(e.to_string()))?
        {
            let index = msg.header.index;
            let is_up = msg.header.flags.contains(&LinkFlag::Up);

            let mut name = String::new();
            let mut is_wireless = false;

            for nla in &msg.attributes {
                if let LinkAttribute::IfName(n) = nla {
                    name = n.clone();
                }
            }

            // Check sysfs for wireless capability (kernel-authoritative)
            if !name.is_empty() {
                let wireless_dir = format!("/sys/class/net/{}/wireless", name);
                is_wireless = std::path::Path::new(&wireless_dir).exists();
            }

            let addresses = self.get_addresses(index).await.unwrap_or_default();

            interfaces.push(InterfaceInfo {
                name,
                index,
                is_up,
                is_wireless,
                addresses,
            });
        }

        debug!("Found {} interfaces", interfaces.len());
        Ok(interfaces)
    }

    /// Get IP addresses for an interface by index.
    async fn get_addresses(&self, index: u32) -> Result<Vec<IpAddr>, NetError> {
        let mut addrs_stream = self
            .handle
            .address()
            .get()
            .set_link_index_filter(index)
            .execute();

        let mut addresses = Vec::new();
        while let Some(msg) = addrs_stream
            .try_next()
            .await
            .map_err(|e| NetError::Netlink(e.to_string()))?
        {
            for nla in &msg.attributes {
                if let AddressAttribute::Address(addr) = nla {
                    addresses.push(*addr);
                }
            }
        }
        Ok(addresses)
    }

    /// Find an interface by name and return its index.
    pub async fn find_interface(&self, name: &str) -> Result<u32, NetError> {
        let interfaces = self.list_interfaces().await?;
        interfaces
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.index)
            .ok_or_else(|| NetError::InterfaceNotFound(name.to_string()))
    }

    /// Find the first wireless interface, if any.
    pub async fn find_wireless(&self) -> Result<Option<InterfaceInfo>, NetError> {
        let interfaces = self.list_interfaces().await?;
        Ok(interfaces.into_iter().find(|i| i.is_wireless))
    }

    /// Find the first non-loopback, non-wireless interface (i.e. ethernet).
    pub async fn find_ethernet(&self) -> Result<Option<InterfaceInfo>, NetError> {
        let interfaces = self.list_interfaces().await?;
        Ok(interfaces
            .into_iter()
            .find(|i| !i.is_wireless && i.name != "lo"))
    }

    /// Bring an interface up.
    pub async fn set_link_up(&self, name: &str) -> Result<(), NetError> {
        let index = self.find_interface(name).await?;
        self.handle
            .link()
            .set(index)
            .up()
            .execute()
            .await
            .map_err(|e| NetError::Netlink(e.to_string()))?;
        debug!("Interface {} set UP", name);
        Ok(())
    }

    /// Bring an interface down.
    pub async fn set_link_down(&self, name: &str) -> Result<(), NetError> {
        let index = self.find_interface(name).await?;
        self.handle
            .link()
            .set(index)
            .down()
            .execute()
            .await
            .map_err(|e| NetError::Netlink(e.to_string()))?;
        debug!("Interface {} set DOWN", name);
        Ok(())
    }

    /// Get the link state (up/down) for an interface.
    pub async fn get_link_state(&self, name: &str) -> Result<bool, NetError> {
        let interfaces = self.list_interfaces().await?;
        interfaces
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.is_up)
            .ok_or_else(|| NetError::InterfaceNotFound(name.to_string()))
    }

    /// Get IP addresses for an interface by name.
    pub async fn get_addresses_by_name(&self, name: &str) -> Result<Vec<IpAddr>, NetError> {
        let index = self.find_interface(name).await?;
        self.get_addresses(index).await
    }

    /// Get the MAC address for an interface (reads from sysfs).
    pub fn get_mac_address(name: &str) -> Option<String> {
        let path = format!("/sys/class/net/{}/address", name);
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "00:00:00:00:00:00")
    }
}
