use std::{collections::HashSet, ffi::OsString, mem::ManuallyDrop, net::Ipv4Addr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use command_wrappers::Interface;
use container::{ContainerFs, ContainerHandle, UnshareContainer};
use nix_helpers::{ContainixFlake, NixStoreItem};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, trace, warn, Level};
use tracing_subscriber::{fmt, fmt::format::FmtSpan, EnvFilter};
use volume_mount::VolumeMount;

mod cli_wrappers;
mod command;
mod command_wrappers;
mod container;
mod init;
mod network_config;
mod nix_helpers;
mod overlayfs;
mod tools;
mod volume_mount;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Build a Nix flake container
    Build(BuildArgs),
    /// Run a Nix flake container
    Run(RunArgs),
    /// Initialize the container (hidden from user)
    #[command(hide = true)]
    ContainerInit,
}

#[derive(Parser, Debug)]
struct BuildArgs {
    /// Nix flake container
    #[arg(short = 'f', long = "flake", value_name = "NIX FILE")]
    flake: ContainixFlake,
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// Nix flake container
    #[arg(short = 'f', long = "flake", value_name = "NIX FLAKE")]
    flake: ContainixFlake,

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

#[instrument(level = "trace", skip_all)]
fn build_command(args: BuildArgs) -> Result<()> {
    let store_item = args.flake.build()?;
    info!(
        "Container built successfully: {}",
        store_item.path().display()
    );
    Ok(())
}

#[instrument(level = "trace", skip_all)]
fn enter_root_ns() -> Result<()> {
    let uid = nix::unistd::getuid();
    let gid = nix::unistd::getgid();
    nix::sched::unshare(
        nix::sched::CloneFlags::CLONE_NEWUSER.union(nix::sched::CloneFlags::CLONE_NEWNS),
    )?;
    std::fs::write("/proc/self/setgroups", "deny")?;
    std::fs::write("/proc/self/uid_map", format!("0 {uid} 1"))?;
    std::fs::write("/proc/self/gid_map", format!("0 {gid} 1"))?;
    Ok(())
}

#[instrument(level = "trace", skip_all)]
fn run_command(args: RunArgs) -> Result<()> {
    info!("Building container {}", args.flake);
    let store_item = args.flake.build()?;
    let closure = store_item.closure()?;
    debug!("Dependency closure: {closure:?}");

    let mut container_fs = ContainerFs::build().rootfs(store_item.path());

    for component in &closure {
        container_fs = container_fs.expose_nix_item(component.path());
    }

    for volume in &args.volumes {
        container_fs = container_fs.add_volume_mount(&volume.host_path, &volume.container_path);
    }

    enter_root_ns()?;
    let container_fs = container_fs.create()?;
    info!("Container root: {}", container_fs.display());

    let mut container = UnshareContainer::new(container_fs)?;
    container.set_keep(args.keep_container);

    let mut config = ContainerConfig {
        flake: store_item.clone(),
        args: args.args,
        interface: Default::default(),
    };

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
        .spawn(
            store_item.path().join("bin").join("containix"),
            ["container-init"],
            build_path_env(&config),
        )
        .context("Spawning container")?;

    if let Some((veth_host, veth_container)) = &interfaces {
        veth_host.up()?;
        veth_container.set_ns(container_pid.pid().to_string())?;
    }

    container_pid
        .wait()
        .context("Waiting for container to exit")?;

    if args.keep_container {
        warn!("Not cleaning up {}", container.root().display());
        _ = ManuallyDrop::new(container);
    }

    Ok(())
}

#[instrument(level = "trace", ret)]
fn setup_network(network: &network_config::NetworkConfig) -> Result<(Interface, Interface)> {
    info!("Configuring network");
    let available_interface_name =
        find_available_veth_name().context("Finding available veth interface")?;
    trace!("Using {available_interface_name} for container interface");
    let (veth_host, veth_container) = Interface::create_veth(
        &available_interface_name,
        format!("{available_interface_name}-peer"),
    )
    .context("Creating veth interface")?;
    trace!(
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
    config.flake.path().join("bin").into()
}

fn main() -> Result<()> {
    fmt()
        .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .with_env_var("CONTAINIX_LOG")
                .from_env()
                .context("Parsing CONTAINIX_LOG")?,
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Build(args) => build_command(args),
        Commands::Run(args) => run_command(args),
        Commands::ContainerInit => init::initialize_container(),
    }
}
