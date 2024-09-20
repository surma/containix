use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    net::Ipv4Addr,
};

use anyhow::{Context, Result};
use clap::Parser;
use command_wrappers::Interface;
use container::ContainerHandle;
use nix_helpers::{NixDerivation, NixStoreItem};
use serde::{Deserialize, Serialize};
use tools::is_container;
use tracing::{info_span, instrument};
use tracing_subscriber::{fmt, EnvFilter};
use volume_mount::VolumeMount;

mod command;
mod command_wrappers;
mod container;
mod init;
mod network_config;
mod nix_helpers;
mod overlayfs;
mod tempdir;
mod tools;
mod volume_mount;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Nix flake container
    #[arg(short = 'f', long = "flake", value_name = "NIX (FLAKE) FILE")]
    flake: NixDerivation,

    /// Arguments to pass to the command.
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,

    /// Volumes to mount into the container.
    #[arg(short = 'v', long = "volume", value_name = "HOST_PATH:CONTAINER_PATH")]
    volumes: Vec<VolumeMount>,

    /// Network configuration for the container.
    #[arg(
        short = 'n',
        long = "network",
        value_name = "HOST_IP+CONTAINER_IP/NETMASK"
    )]
    network: Option<network_config::NetworkConfig>,

    /// Keep the container root directory after the command has run.
    #[arg(short = 'k', long = "keep")]
    keep_container: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerConfig {
    pub flake: NixStoreItem,
    pub args: Vec<String>,
    pub interface: Option<InterfaceConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InterfaceConfig {
    name: String,
    address: Ipv4Addr,
    netmask: Ipv4Addr,
}

fn create_container() -> Result<()> {
    tracing::info!("Starting containix");
    let args = CliArgs::parse();
    let store_item = build_container(&args)?;
    let closure = store_item.closure()?;

    let mut container = container::Container::temp_container()?;
    container.set_keep(args.keep_container);
    tracing::info!("Container root: {}", container.root().display());

    let mut config = ContainerConfig {
        flake: store_item.clone(),
        args: args.args,
        interface: Default::default(),
    };

    tracing::info!("Mounting components");
    for component in &closure {
        container.bind_mount(component.as_path(), component.as_path(), true)?;
    }
    tracing::info!("Mounting volumes");
    for volume in &args.volumes {
        container.bind_mount(&volume.host_path, &volume.container_path, false)?;
    }
    let interfaces = if let Some(network) = &args.network {
        container.set_netns(true);
        let (host_veth, container_veth) = setup_network(network)?;
        config.interface = Some(InterfaceConfig {
            name: container_veth.name().to_string(),
            address: network.container_address,
            netmask: network.netmask,
        });
        Some((host_veth, container_veth))
    } else {
        None
    };

    serde_json::to_writer_pretty(
        std::fs::File::create(container.root().join("containix.config.json"))
            .context("Creating container config file")?,
        &config,
    )
    .context("Writing container config")?;

    let mut container_pid = container
        .spawn("/containix", &[] as &[&OsStr], build_path_env(&config))
        .context("Spawning container")?;

    if let Some((veth_host, veth_container)) = &interfaces {
        veth_host.up()?;
        veth_container.set_ns(container_pid.pid().to_string())?;
    }

    container_pid
        .wait()
        .context("Waiting for container to exit")?;

    Ok(())
}

#[instrument(level = "trace", skip_all, ret)]
fn build_container(args: &CliArgs) -> Result<NixStoreItem> {
    tracing::info!("Building flake container...");
    let store_item = args.flake.build().context("Building flake container")?;
    Ok(store_item)
}

#[instrument(level = "trace", ret)]
fn setup_network(network: &network_config::NetworkConfig) -> Result<(Interface, Interface)> {
    tracing::info!("Configuring network");
    let available_interface_name =
        find_available_veth_name().context("Finding available veth interface")?;
    tracing::trace!("Using {available_interface_name} for container interface");
    let (veth_host, veth_container) = Interface::create_veth(
        &available_interface_name,
        format!("{available_interface_name}-peer"),
    )
    .context("Creating veth interface")?;
    tracing::trace!(
        "Setting host interface ({}) address to {}/{}",
        veth_host.name(),
        network.host_address.to_string(),
        network.netmask.to_string()
    );
    veth_host
        .set_address(&network.host_address, &network.netmask)
        .context("Setting host interface address")?;

    // We don’t need to clean up these interfaces.
    // The container interface will be cleaned up when the container exits,
    // as the kernel will remove the namespace.
    // The host interface will be cleaned up as a veth pair is deleted when one end is deleted.
    Ok((veth_host, veth_container))
}

fn find_available_veth_name() -> Result<String> {
    let interface_names: HashSet<_> = Interface::all()?
        .into_iter()
        .map(|i| i.name().to_string())
        .collect();
    let Some(available_interface_index) =
        (0..100).find(|i| !interface_names.contains(&format!("veth{i}")))
    else {
        anyhow::bail!("No available names for veth interface");
    };
    Ok(format!("veth{available_interface_index}"))
}

fn build_path_env(config: &ContainerConfig) -> OsString {
    config.flake.path.join("bin").into()
}

fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let span = info_span!(
        "containix",
        mode = if is_container() { "container" } else { "host" }
    );
    let _guard = span.enter();

    if is_container() {
        init::initialize_container()
    } else {
        create_container()
    }
}
