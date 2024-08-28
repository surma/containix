use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use anyhow::{Context, Result};
use clap::Parser;
use nix_helpers::{NixComponent, NixStoreItem};
use serde::{Deserialize, Serialize};
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

    /// Working directory inside the container.
    #[arg(short, long, value_name = "PATH", default_value = "/")]
    workdir: PathBuf,

    /// Keep the container root directory after the command has run.
    #[arg(short = 'k', long = "keep")]
    keep_container: bool,

    /// The command to run in the container.
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
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

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContainerConfig {
    pub command: Vec<String>,
    pub exposed_components: Vec<NixStoreItem>,
}

fn create_container() -> Result<()> {
    tracing::info!("Starting containix");
    let args = CliArgs::parse();
    tracing::info!("Realising components");
    let exposed_components = args
        .exposed_components
        .iter()
        .map(|c| c.clone().realise())
        .collect::<Result<HashSet<_>>>()?;

    let mut container = container::Container::temp_container()?;
    container.set_keep(args.keep_container);
    tracing::info!("Container root: {}", container.root().display());

    let config = ContainerConfig {
        command: args.command,
        exposed_components: exposed_components
            .iter()
            .map(|c| {
                c.as_store()
                    .expect("Guaranteed by NixComponent::realise()")
                    .clone()
            })
            .collect(),
    };
    serde_json::to_writer_pretty(
        std::fs::File::create(container.root().join("containix.config.json"))
            .context("Creating container config file")?,
        &config,
    )
    .context("Writing container config")?;

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
    container_cmd.current_dir(&args.workdir);

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

fn resolve_command<I: AsRef<Path>>(
    command: impl AsRef<OsStr>,
    exposed_components: impl IntoIterator<Item = I>,
) -> Option<PathBuf> {
    let command = command.as_ref();
    exposed_components
        .into_iter()
        .map(|c| c.as_ref().join(command))
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

fn initialize_container() -> Result<()> {
    tracing::info!("Starting containix in container");
    let config_path =
        std::fs::File::open("/containix.config.json").context("Opening container config")?;
    let config: ContainerConfig =
        serde_json::from_reader(config_path).context("Parsing container config")?;

    let component_paths: Vec<_> = config
        .exposed_components
        .iter()
        .map(|c| c.as_path().join("bin").as_os_str().to_os_string())
        .collect();
    let path_var = component_paths.join(&OsString::from(":"));

    let Some(command_name) = config.command.first() else {
        anyhow::bail!("No command to run");
    };

    let Some(command) = resolve_command(command_name, &component_paths) else {
        anyhow::bail!("Command {} not found in exposed components", command_name);
    };

    let err = Command::new(command)
        .args(&config.command[1..])
        .env("PATH", path_var)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .exec();

    Err(err.into())
}

fn is_container() -> bool {
    std::env::var("CONTAINIX_CONTAINER").is_ok()
}

fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    if is_container() {
        initialize_container()
    } else {
        create_container()
    }
}
