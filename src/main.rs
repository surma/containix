use std::{collections::HashSet, ffi::OsString, mem::ManuallyDrop, net::Ipv4Addr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use container::{ContainerFs, ContainerFsBuilder, ContainerHandle, UnshareContainer};
use nix_helpers::{ContainixFlake, NixStoreItem};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, trace, warn, Level};
use tracing_subscriber::{fmt, fmt::format::FmtSpan, EnvFilter};
use unshare::{UnshareEnvironmentBuilder, UnshareNamespaces};
use volume_mount::VolumeMount;

mod cli_wrappers;
mod command;
mod container;
mod mount;
mod nix_helpers;
mod path_ext;
mod unshare;
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

    /// Keep the container root directory after the command has run.
    #[arg(short = 'k', long = "keep")]
    keep_container: bool,
}

#[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
fn containix_build(args: BuildArgs) -> Result<()> {
    let store_item = args.flake.build()?;
    info!(
        "Container built successfully: {}",
        store_item.path().display()
    );
    Ok(())
}

#[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
fn enter_root_ns() -> Result<()> {
    let mut builder = UnshareEnvironmentBuilder::default();
    builder
        .namespace(UnshareNamespaces::User)
        .namespace(UnshareNamespaces::Mount)
        .map_current_user_to_root();
    if let Some(mut child) = builder.enter()? {
        warn!("Entering root namespace created a child when it shouldn’t.");
        std::process::exit(child.wait()?);
    }

    Ok(())
}

#[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
fn containix_run(args: RunArgs) -> Result<()> {
    info!("Building container {}", args.flake);
    let store_item = args.flake.build().context("Building container flake")?;
    let closure = store_item
        .closure()
        .context("Computing transitive closure")?;
    debug!(
        "Dependency closure: {}",
        closure
            .iter()
            .map(|c| c.name())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut container_fs = ContainerFsBuilder::default();
    for component in &closure {
        container_fs.nix_component(component.path());
    }

    for volume in &args.volumes {
        container_fs.volume(volume.clone());
    }

    enter_root_ns()?;
    let container_fs = container_fs.build().context("Building container fs")?;
    info!("Container root: {}", container_fs.display());

    let mut container =
        UnshareContainer::new(container_fs).context("Entering container namespace")?;
    container.set_keep(args.keep_container);

    let invocation = if args.args.is_empty() {
        let cmd = store_item.path().join("bin").join("containix-entry-point");
        let Some(cmd) = cmd.to_str() else {
            anyhow::bail!("Container flake name contains invalid utf-8");
        };
        vec![cmd.to_string()]
    } else {
        args.args.clone()
    };
    trace!("Spawning container with command: {:?}", invocation);
    let mut container_pid = container
        .spawn(
            invocation
                .get(0)
                .expect("guaranteed to have at least 1 element by code above"),
            &invocation[1..],
            store_item.path().join("bin"),
        )
        .context("Spawning container")?;

    container_pid
        .wait()
        .context("Waiting for container to exit")?;

    if args.keep_container {
        warn!("Not cleaning up {}", container.root().display());
        _ = ManuallyDrop::new(container);
    }

    Ok(())
}

fn main() -> Result<()> {
    fmt()
        .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT)
        .with_target(false)
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
        Commands::Build(args) => containix_build(args),
        Commands::Run(args) => containix_run(args),
    }
}
