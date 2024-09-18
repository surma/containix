use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    ffi::{OsStr, OsString},
    net::Ipv4Addr,
    path::PathBuf,
    str::FromStr,
};

use anyhow::{Context, Result};
use clap::{ArgGroup, Parser};
use command_wrappers::Interface;
use container::ContainerHandle;
use nix_helpers::{combine_closures, NixDerivation, NixStoreItem};
use serde::{Deserialize, Serialize};
use tools::{is_container, NIXPKGS_24_05};
use tracing::info_span;
use tracing_subscriber::{fmt, EnvFilter};

mod command;
mod command_wrappers;
mod container;
mod init;
mod network_config;
mod nix_helpers;
mod tools;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Nix derivations that should be built and made available to the container.
    #[arg(short = 'b', long = "build", value_name = "NIX (FLAKE) FILE")]
    expose: Vec<NixDerivation>,

    /// Convenience flag to expose packages from nixpkgs to the container
    #[arg(short = 'p', long = "package", value_name = "NIXPKG PACKAGE NAME")]
    packages: Vec<String>,

    /// The command to run in the container.
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,

    /// Volumes to mount into the container.
    #[arg(short = 'v', long = "volume", value_name = "HOST_PATH:CONTAINER_PATH")]
    volumes: Vec<VolumeMount>,

    /// Network configuration for the container.
    #[arg(
        short = 'n',
        long = "network",
        value_name = "HOST_IP!CONTAINER_IP/NETMASK"
    )]
    network: Option<network_config::NetworkConfig>,

    /// Working directory inside the container.
    #[arg(short, long, value_name = "PATH", default_value = "/")]
    workdir: PathBuf,

    /// Keep the container root directory after the command has run.
    #[arg(short = 'k', long = "keep")]
    keep_container: bool,

    #[command(flatten)]
    #[command(next_help_heading = "Dangerous Flags")]
    dangerous_flags: DangerousFlags,
}

#[derive(Debug, Parser)]
struct DangerousFlags {
    /// Overwrite the flake expression used for nixpkgs packages.
    #[arg(
            long,
            value_name = "NIXPKG SOURCE",
            default_value = NIXPKGS_24_05,
        )]
    nixpkgs: String,

    /// Do not expose any tools by default (you must provide your own).
    #[arg(long, default_value_t = false)]
    no_default_expose: bool,
}

#[derive(Debug, Clone)]
struct VolumeMount {
    host_path: PathBuf,
    container_path: PathBuf,
}

impl FromStr for VolumeMount {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let Some((host_path, container_path)) = s.split_once(':') else {
            anyhow::bail!(
                "Volume mount must be of the form <HOST PATH>:<CONTAINER PATH>, got: {s}"
            );
        };
        Ok(VolumeMount {
            host_path: host_path.into(),
            container_path: container_path.into(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContainerConfig {
    pub command: Vec<String>,
    pub exposed_components: Vec<NixStoreItem>,
    pub workdir: PathBuf,
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
    let store_items = build_inputs(&args)?;
    let closure = combine_closures(&store_items)?;

    let mut container = container::Container::temp_container()?;
    container.set_keep(args.keep_container);
    tracing::info!("Container root: {}", container.root().display());

    let mut config = ContainerConfig {
        command: args.command,
        exposed_components: store_items,
        workdir: args.workdir,
        ..Default::default()
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

fn build_inputs(args: &CliArgs) -> Result<Vec<NixStoreItem>> {
    tracing::info!("Assembling & building inputs");
    let build_inputs: Vec<NixDerivation> = args
        .packages
        .iter()
        .map(|p| NixDerivation::package_from_flake(p, &args.dangerous_flags.nixpkgs))
        .chain(args.expose.clone().into_iter())
        .collect();
    tracing::trace!("Flakes to build: {build_inputs:?}");
    let store_items = build_inputs
        .into_iter()
        .map(|flake| {
            tracing::info!("Building input flake: {}", flake);
            flake.build().context("Building input flake")
        })
        .collect::<Result<Vec<_>>>()?;
    tracing::trace!("Flakes built: {store_items:?}");
    Ok(store_items)
}

fn setup_network(network: &network_config::NetworkConfig) -> Result<(Interface, Interface)> {
    tracing::info!("Configuring network");
    let available_interface_name =
        find_available_veth_name().context("Finding available veth interface")?;
    tracing::info!("Using {available_interface_name} for container interface");
    let (veth_host, veth_container) = Interface::create_veth(
        &available_interface_name,
        format!("{available_interface_name}-peer"),
    )
    .context("Creating veth interface")?;
    tracing::trace!(
        "Setting host interface address to {}/{}",
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
    let component_paths: Vec<_> = config
        .exposed_components
        .iter()
        .map(|c| c.as_path().join("bin").as_os_str().to_os_string())
        .collect();

    component_paths.join(&OsString::from(":"))
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
