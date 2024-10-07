use std::mem::ManuallyDrop;

use anyhow::{bail, Context, Result};
use clap::Parser;
use containix::command::ChildProcess;
use containix::container::{ContainerBuilder, ContainerFsBuilder};
use containix::env::EnvVariable;
use containix::host_tools::setup_host_tools;
use containix::nix_helpers::ContainixFlake;
use containix::ports::PortMapping;
use containix::unshare::{UnshareEnvironmentBuilder, UnshareNamespaces};
use containix::volume_mount::VolumeMount;
use tracing::{debug, info, instrument, trace, warn, Level};
use tracing_subscriber::{fmt, fmt::format::FmtSpan, EnvFilter};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Nix flake container
    #[arg(short = 'f', long = "flake", value_name = "NIX FLAKE")]
    flake: ContainixFlake,

    /// Arguments to pass to the container entry point.
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,

    /// Environment variables to set in the container.
    #[arg(short = 'e', long = "env", value_name = "KEY=VALUE")]
    env: Vec<EnvVariable>,

    /// Set the uid of the user running the container.
    // #[arg(long = "set-uid", value_name = "UID")]
    // set_uid: Option<u32>,

    /// Set the gid of the user running the container.
    // #[arg(long = "set-gid", value_name = "GID")]
    // set_gid: Option<u32>,

    /// Volumes to mount into the container.
    #[arg(short = 'v', long = "volume", value_name = "HOST_PATH:CONTAINER_PATH")]
    volumes: Vec<VolumeMount>,

    /// Ports to expose to the host.
    #[arg(short = 'p', long = "port", value_name = "HOST_PORT:CONTAINER_PORT")]
    ports: Vec<PortMapping>,

    /// Keep the container root directory after the command has run.
    #[arg(short = 'k', long = "keep")]
    keep_container: bool,

    /// Path to host tools.
    #[arg(
        long = "host-tools",
        value_name = "PATH or FLAKE",
        default_value = "github:surma/containix#host-tools"
    )]
    host_tools: String,

    /// Mount the entire Nix store into the container, rather than just the transitive closure.
    #[arg(long = "full-nix-store")]
    full_nix_store: bool,

    /// (Nix passthru:) Consider all previously downloaded files out-of-date.
    #[arg(long = "refresh", default_value_t = false)]
    refresh: bool,
}

#[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
fn enter_root_ns() -> Result<()> {
    let mut builder = UnshareEnvironmentBuilder::default();
    builder
        .namespace(UnshareNamespaces::User)
        .namespace(UnshareNamespaces::Mount)
        .map_current_user_to_root();
    builder.enter()?;
    Ok(())
}

#[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
fn containix_run(args: Cli) -> Result<()> {
    setup_host_tools(&args.host_tools, args.refresh)?;
    info!("Building container {}", args.flake);
    let store_item = args
        .flake
        .build(|cmd_args| {
            cmd_args.refresh(args.refresh);
        })
        .context("Building container flake")?;
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
    if args.full_nix_store {
        container_fs.volume(VolumeMount::read_only("/nix/store", "/nix/store"));
    } else {
        for component in &closure {
            container_fs.nix_component(component.path());
        }
    }

    for volume in &args.volumes {
        container_fs.volume(volume.clone());
    }

    enter_root_ns()?;
    let container_fs = container_fs.build().context("Building container fs")?;
    let root = container_fs.as_ref().to_path_buf();
    info!("Container root: {}", root.display());

    let mut container_builder = ContainerBuilder::default()
        .root(container_fs)
        .ports(args.ports)
        .env("PATH", store_item.path().join("bin"))
        .envs(args.env);

    let cmd = store_item.path().join("bin").join("containix-entry-point");
    let Some(cmd) = cmd.to_str() else {
        bail!("Container flake name contains invalid utf-8");
    };
    container_builder = container_builder.command(cmd).args(args.args);

    // if let Some(uid) = args.set_uid {
    //     container_builder = container_builder.uid(uid);
    // }
    // if let Some(gid) = args.set_gid {
    //     container_builder = container_builder.gid(gid);
    // }

    let mut container_handle = container_builder.spawn().context("Spawning container")?;
    trace!("Container started with PID {}", container_handle.pid());

    container_handle
        .wait()
        .context("Waiting for container to exit")?;

    if args.keep_container {
        warn!("Not cleaning up {}", container_handle.root().display());
        _ = ManuallyDrop::new(container_handle);
    }

    Ok(())
}

fn main() -> Result<()> {
    fmt()
        .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT)
        .with_target(true)
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
    containix_run(cli)
}
