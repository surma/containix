use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::PathBuf,
    str::FromStr,
};

use anyhow::{Context, Result};
use clap::Parser;
use nix_helpers::NixComponent;
use tracing_subscriber::{fmt, EnvFilter};

mod container;
mod nix_helpers;
mod unix_helpers;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Volumes to mount into the container.
    #[arg(short = 'v', long = "volume", value_name = "HOST PATH:CONTAINER PATH")]
    volumes: Vec<VolumeMount>,

    /// Additional nix components to bind mount into the container.
    #[arg(short = 'e', long = "expose", value_name = "NIX STORE PATH")]
    exposed_components: Vec<NixComponent>,

    /// Keep the container root directory after the command has run.
    #[arg(short = 'k', long = "keep")]
    keep_container: bool,

    #[arg(long, hide = true, default_value_t = std::env::var("CONTAINIX_CONTAINER").is_ok())]
    container_mode: bool,

    /// The command to run in the container.
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[derive(Parser, Debug)]
enum Command {
    CreateContainer(Args),
    InitializeContainer(Args),
}

#[derive(Debug, Clone)]
struct VolumeMount {
    host_path: PathBuf,
    container_path: PathBuf,
}

impl FromStr for VolumeMount {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<_> = s.splitn(2, ':').collect();
        let &[host_path, container_path] = &parts[..] else {
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
#[derive(Parser, Debug)]
struct Args {}

#[derive(Parser, Debug)]
struct InitializeContainerArgs {}

fn combine_closures(
    exposed_components: impl IntoIterator<Item = NixComponent>,
) -> Result<HashSet<NixComponent>> {
    let mut closure = HashSet::new();
    for component in exposed_components {
        closure.extend(component.closure()?);
        closure.insert(component.clone());
    }
    Ok(closure)
}

fn create_container(args: CliArgs) -> Result<()> {
    tracing::info!("Realising components");
    let exposed_components = args
        .exposed_components
        .iter()
        .map(|c| c.clone().realise())
        .collect::<Result<HashSet<_>>>()?;

    let mut container = container::Container::temp_container()?;
    container.set_keep(args.keep_container);
    tracing::info!("Container root: {}", container.root().display());

    let closure = combine_closures(exposed_components.clone())?
        .into_iter()
        .map(|c| c.store_path())
        .collect::<Result<Vec<_>, _>>()?;
    tracing::info!("Mounting components");
    for component in &closure {
        container.bind_mount(component, component, true)?;
    }
    tracing::info!("Mounting volumes");
    for volume in &args.volumes {
        container.bind_mount(&volume.host_path, &volume.container_path, false)?;
    }

    let mut container_cmd = std::process::Command::new("/containix");
    container_cmd.args(std::env::args_os().skip(1));
    container_cmd.env("CONTAINIX_CONTAINER", "1");
    container_cmd.current_dir("/");

    container_cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let container_pid = container
        .spawn(container_cmd)
        .context("Spawning container")?;

    container_pid
        .wait()
        .context("Waiting for container to exit")?;

    Ok(())
}

fn resolve_command<'a>(
    command: impl AsRef<OsStr>,
    exposed_components: impl IntoIterator<Item = &'a NixComponent>,
) -> Option<PathBuf> {
    let command = command.as_ref();
    exposed_components
        .into_iter()
        .map(|c| {
            c.store_path()
                .expect("Guaranteed by calling realise()")
                .join("bin")
                .join(command)
        })
        .find(|p| p.exists())
}

fn build_path_var<'a>(exposed_components: impl IntoIterator<Item = &'a NixComponent>) -> OsString {
    let path_var = exposed_components
        .into_iter()
        .map(|p| {
            p.store_path()
                .expect("Guaranteed by calling realise()")
                .join("bin")
                .as_os_str()
                .to_os_string()
        })
        .collect::<Vec<_>>()
        .join(OsString::from(":").as_os_str());
    path_var
}

fn expose_component_paths(
    container_cmd: &mut std::process::Command,
    exposed_components: HashSet<NixComponent>,
) -> OsString {
    let container_cmd: &mut std::process::Command = container_cmd;
    container_cmd.current_dir("/");
    let path_var = exposed_components
        .iter()
        .map(|p| {
            p.store_path()
                .expect("Guaranteed by calling realise()")
                .join("bin")
                .as_os_str()
                .to_os_string()
        })
        .collect::<Vec<_>>()
        .join(OsString::from(":").as_os_str());

    tracing::trace!("Setting $PATH: {path_var:?}");
    container_cmd.env_clear().env("PATH", &path_var);
    path_var
}

fn initialize_container(args: CliArgs) -> Result<()> {
    println!("Initializing container");
    Ok(())
}

fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    tracing::trace!("Starting containix");
    let args = CliArgs::parse();

    if args.container_mode {
        initialize_container(args)
    } else {
        create_container(args)
    }
}
