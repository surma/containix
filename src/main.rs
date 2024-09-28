use std::{collections::HashSet, ffi::OsString, mem::ManuallyDrop, net::Ipv4Addr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use container::{ContainerFs, ContainerHandle, UnshareContainer};
use nix_helpers::{ContainixFlake, NixStoreItem};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, trace, warn, Level};
use tracing_subscriber::{fmt, fmt::format::FmtSpan, EnvFilter};
use volume_mount::VolumeMount;

mod cli_wrappers;
mod command;
mod container;
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

#[instrument(level = "trace", skip_all)]
fn containix_build(args: BuildArgs) -> Result<()> {
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
fn containix_run(args: RunArgs) -> Result<()> {
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

    let invocation = if args.args.is_empty() {
        let cmd = store_item.path().join("bin").join("containix-entry-point");
        let Some(cmd) = cmd.to_str() else {
            anyhow::bail!("Container flake name contains invalid utf-8");
        };
        vec![cmd.to_string()]
    } else {
        args.args.clone()
    };

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
