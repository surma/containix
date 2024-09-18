use std::{net::Ipv4Addr, str::FromStr};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub host_address: Ipv4Addr,
    pub container_address: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

impl FromStr for NetworkConfig {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let Some((addresses, netmask)) = s.split_once('/') else {
            anyhow::bail!("Network config must be of the form <HOST_ADDRESS>!<CONTAINER_ADDRESS>/<NETMASK>, got: {s}");
        };
        let Some((host, container)) = addresses.split_once('!') else {
            anyhow::bail!("Network config must be of the form <HOST_ADDRESS>!<CONTAINER_ADDRESS>/<NETMASK>, got: {s}");
        };
        let netmask = if netmask.contains('.') {
            netmask.parse()?
        } else {
            let netmask = netmask.parse::<u32>()?;
            Ipv4Addr::from_bits(!((1 << (32 - netmask)) - 1))
        };
        Ok(NetworkConfig {
            host_address: host.parse()?,
            container_address: container.parse()?,
            netmask,
        })
    }
}
