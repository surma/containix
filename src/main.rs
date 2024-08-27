use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Parser;
use derive_more::derive::From;
use scopeguard::ScopeGuard;

mod nix;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Host(HostArgs),
    Container(ContainerArgs),
}

// #[derive(Debug, Clone)]
// struct NetworkTunnel {
//     host_address: std::net::IpNet,
//     container_address: std::net::IpAddr,

// }

#[derive(Parser, Debug)]
struct HostArgs {
    #[arg(short, long, value_name = "DIR")]
    root_dir: Option<PathBuf>,

    #[arg(short, long, value_name = "DIR")]
    network: Option<PathBuf>,

    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[derive(Parser, Debug)]
struct ContainerArgs {
    // Define container-specific flags here
    // For example:
    // #[arg(short, long)]
    // image: String,
}

fn handle_host(args: HostArgs) -> Result<()> {
    let [command, extra_args @ ..] = &args.command[..] else {
        anyhow::bail!("No command given");
    };
    let Some(command) = nix::find_nix_store_path(command) else {
        anyhow::bail!("Command not found in nix store");
    };
    tracing::trace!("Resolved command path: {command:?}");

    let guard = if let Some(root_dir) = &args.root_dir {
        let closure = nix::get_nix_closure(&command)?;
        tracing::trace!("Nix closure: {closure:?}");
        Some(bind_mount_all::<fn(Vec<PathBuf>) -> ()>(root_dir, closure)?)
    } else {
        None
    };

    // Execute the command in the new root environment using unshare
    let mut child_process = std::process::Command::new("unshare");
    // command
    //     .arg("--mount")
    //     .arg("--uts")
    //     .arg("--ipc")
    //     .arg("--pid")
    //     .arg("--fork")
    //     .arg("--mount-proc");

    if let Some(root_dir) = args.root_dir.as_ref() {
        child_process.arg("--root").arg(root_dir);
    }

    child_process.arg(&command);
    child_process.args(extra_args);

    child_process
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let status = child_process.status()?;

    if !status.success() {
        anyhow::bail!("Command failed with exit code: {}", status);
    }

    // The guard will be dropped here, unmounting the bind mounts

    Ok(())
}
fn bind_guard_cleanup(dirs: Vec<PathBuf>) {
    for dir in dirs {
        if let Err(e) = std::process::Command::new("umount").arg(&dir).status() {
            tracing::error!("Failed to unmount {}: {}", dir.display(), e);
        }
    }
}
/// Bind mounts all paths in the nix closure to the root directory.
///
/// This function creates a directory structure in the root directory that mirrors the
/// nix store, binds the paths to the store, and returns a guard that will unmount the paths
/// when dropped.
fn bind_mount_all<F: FnOnce(Vec<PathBuf>) -> ()>(
    root_dir: impl AsRef<Path>,
    closure: Vec<PathBuf>,
) -> Result<ScopeGuard<Vec<PathBuf>, impl FnOnce(Vec<PathBuf>) -> ()>> {
    let nix_store_dir = root_dir.as_ref().join("nix").join("store");
    std::fs::create_dir_all(&nix_store_dir)?;

    let mut guard = scopeguard::guard(Vec::<PathBuf>::new(), bind_guard_cleanup);

    for path in closure {
        let target_dir = nix_store_dir.join(path.file_name().unwrap());
        std::fs::create_dir_all(&target_dir)?;

        let status = std::process::Command::new("mount")
            .args(&["-o", "bind,ro"])
            .arg(&path)
            .arg(&target_dir)
            .status()?;

        if !status.success() {
            anyhow::bail!("Failed to bind mount {}", path.display());
        }

        guard.push(target_dir);
    }
    Ok(guard)
}

fn handle_container(args: ContainerArgs) -> Result<()> {
    todo!()
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Host(host_args) => handle_host(host_args),
        Command::Container(container_args) => handle_container(container_args),
    }
}
