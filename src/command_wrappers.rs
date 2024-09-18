use anyhow::{Context, Result};
use derive_more::derive::Deref;
use enum_as_inner::EnumAsInner;
use serde::Deserialize;
use std::{
    ffi::OsString,
    net::{Ipv4Addr, Ipv6Addr},
    path::Path,
    sync::LazyLock,
};

use crate::{command::run_command, tools::TOOLS};

pub fn bind_mount(src: impl AsRef<Path>, dst: impl AsRef<Path>, read_only: bool) -> Result<()> {
    static MOUNT: LazyLock<OsString> = LazyLock::new(|| TOOLS.get("mount").unwrap().path.clone());
    let src = src.as_ref();
    let dst = dst.as_ref();

    let mut command = std::process::Command::new(&*MOUNT);
    command.arg("-o");
    if read_only {
        command.arg("bind,ro");
    } else {
        command.arg("bind");
    }
    command.arg(src);
    command.arg(dst);
    run_command(command)?;

    Ok(())
}

pub fn unmount(path: impl AsRef<Path>) -> Result<()> {
    static UMOUNT: LazyLock<OsString> = LazyLock::new(|| TOOLS.get("umount").unwrap().path.clone());
    let path = path.as_ref();
    let mut command = std::process::Command::new(&*UMOUNT);
    command.arg(path);
    run_command(command)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct Interface {
    #[serde(rename = "ifindex")]
    index: u32,
    #[serde(rename = "ifname")]
    name: String,
    #[serde(rename = "address")]
    mac_address: String,
    #[serde(rename = "addr_info")]
    pub addresses: Vec<InterfaceAddress>,
}

#[derive(Debug, Deserialize, EnumAsInner)]
#[serde(tag = "family")]
pub enum InterfaceAddress {
    #[serde(rename = "inet")]
    Ipv4(Ipv4Address),
    #[serde(rename = "inet6")]
    Ipv6(Ipv6Address),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize, Deref)]
pub struct Ipv4Address {
    #[deref]
    pub local: Ipv4Addr,
    pub prefixlen: u32,
    pub broadcast: Option<Ipv4Addr>,
}
#[derive(Debug, Deserialize, Deref)]
pub struct Ipv6Address {
    #[deref]
    pub local: Ipv6Addr,
    pub prefixlen: u32,
    pub broadcast: Option<Ipv6Addr>,
}

static IP: LazyLock<OsString> = LazyLock::new(|| TOOLS.get("ip").unwrap().path.clone());

impl Interface {
    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn update(&mut self) -> Result<()> {
        let Some(interface) = Interface::by_name(self.name())? else {
            anyhow::bail!("Interface with index {} not found", self.index());
        };
        *self = interface;
        Ok(())
    }

    pub fn by_name(name: impl AsRef<str>) -> Result<Option<Interface>> {
        let name = name.as_ref();
        Ok(Interface::all()?.into_iter().find(|i| i.name() == name))
    }

    pub fn all() -> Result<Vec<Interface>> {
        let mut command = std::process::Command::new(&*IP);
        command.arg("-json");
        command.arg("addr");
        let output = run_command(command)?;
        serde_json::from_slice(&output.stdout).context("Failed to parse ip output")
    }

    pub fn create_veth(
        name: impl AsRef<str>,
        peer: impl AsRef<str>,
    ) -> Result<(Interface, Interface)> {
        let name = name.as_ref();
        let peer = peer.as_ref();
        tracing::trace!("Creating veth pair {name} and {peer}");
        let mut command = std::process::Command::new(&*IP);
        command.arg("link");
        command.arg("add");
        command.arg(name);
        command.arg("type");
        command.arg("veth");
        command.arg("peer");
        command.arg(peer);
        run_command(command)?;

        let Some(i) = Interface::by_name(name)? else {
            anyhow::bail!("Interface {name} not found");
        };
        let Some(j) = Interface::by_name(peer)? else {
            anyhow::bail!("Interface {peer} not found");
        };
        Ok((i, j))
    }

    pub fn delete(&self) -> Result<()> {
        tracing::trace!("Deleting interface {}", self.name);
        let mut command = std::process::Command::new(&*IP);
        command.arg("link");
        command.arg("delete");
        command.arg(&self.name);
        run_command(command)?;
        Ok(())
    }

    pub fn set_address(
        &self,
        address: &std::net::Ipv4Addr,
        netmask: &std::net::Ipv4Addr,
    ) -> Result<()> {
        tracing::trace!(
            "Setting address {} and netmask {} for {}",
            address,
            netmask,
            self.name
        );
        let mut command = std::process::Command::new(&*IP);
        command.arg("addr");
        command.arg("add");
        command.arg(format!("{}/{}", address, netmask));
        command.arg("dev");
        command.arg(&self.name);
        run_command(command)?;
        Ok(())
    }

    pub fn address(&self) -> Result<&Ipv4Address> {
        let Some(address) = self.addresses.iter().find_map(|addr| addr.as_ipv4()) else {
            anyhow::bail!("Interface {} has no IPv4 address", self.name);
        };
        Ok(address)
    }

    pub fn set_ns(&self, ns: impl AsRef<Path>) -> Result<()> {
        tracing::trace!(
            "Setting namespace for {} to {}",
            self.name,
            ns.as_ref().display()
        );
        let mut command = std::process::Command::new(&*IP);
        command.arg("link");
        command.arg("set");
        command.arg(&self.name);
        command.arg("netns");
        command.arg(ns.as_ref());
        run_command(command)?;
        Ok(())
    }

    pub fn set_state(&self, up: bool) -> Result<()> {
        tracing::trace!(
            "Setting state for {} to {}",
            self.name,
            if up { "up" } else { "down" }
        );
        let mut command = std::process::Command::new(&*IP);
        command.arg("link");
        command.arg("set");
        command.arg(&self.name);
        command.arg(if up { "up" } else { "down" });
        run_command(command)?;
        Ok(())
    }

    pub fn up(&self) -> Result<()> {
        self.set_state(true)
    }

    pub fn down(&self) -> Result<()> {
        self.set_state(false)
    }
}
