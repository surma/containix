use anyhow::{Context, Result};
use derive_more::derive::Deref;

use std::{
    ffi::OsStr,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use crate::{
    nix_helpers::Nixpkgs,
    tools::UTIL_COMPONENT,
    unix_helpers::{bind_mount, unmount},
};

#[derive(Debug)]
pub struct Container {
    root: PathBuf,
    mounts: Vec<PathBuf>,
    keep: bool,
}

impl Container {
    pub fn new(root: PathBuf) -> Result<Self> {
        copy_containix(&root)?;
        Ok(Self {
            root,
            mounts: Vec::new(),
            keep: false,
        })
    }

    pub fn set_keep(&mut self, keep: bool) {
        self.keep = keep;
    }

    pub fn temp_container() -> Result<Self> {
        let container_id = uuid::Uuid::new_v4().to_string();
        let temp_dir = std::env::temp_dir().join("containix").join(container_id);
        std::fs::create_dir_all(&temp_dir).context("Creating temporary directory")?;
        Self::new(temp_dir)
    }

    pub fn bind_mount(
        &mut self,
        src: impl AsRef<Path>,
        target: impl AsRef<Path>,
        read_only: bool,
    ) -> Result<()> {
        let src = src.as_ref();
        let target = target.as_ref();
        let target = target.strip_prefix("/").unwrap_or(target);
        let target_dir = self.root.join(target);
        tracing::trace!("Binding mount {src:?} -> {target_dir:?}");
        std::fs::create_dir_all(&target_dir).context("Creating directory for bind mount")?;

        bind_mount(src, target_dir, read_only).context("Mounting")?;
        self.mounts.push(target.to_path_buf());
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn spawn(
        &self,
        command: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Result<impl ContainerHandle> {
        std::fs::create_dir_all(self.root().join("proc")).context("Creating proc directory")?;

        let mut unshare = std::process::Command::new(UTIL_COMPONENT.join("bin").join("unshare"));
        unshare.arg("--root");
        unshare.arg(self.root());
        unshare.arg("--fork");
        unshare.arg("-m");
        unshare.arg("-p");
        unshare.arg("--mount-proc=/proc");
        unshare.arg(command.as_ref());
        unshare.args(args);
        unshare.env_clear();
        unshare.env("CONTAINIX_CONTAINER", "1");
        unshare.stdout(std::process::Stdio::inherit());
        unshare.stderr(std::process::Stdio::inherit());
        unshare.stdin(std::process::Stdio::inherit());
        let child = unshare.spawn()?;
        Ok(child)
    }
}

fn copy_containix(root: impl AsRef<Path>) -> Result<()> {
    let target = root.as_ref().join("containix");

    std::fs::copy("/proc/self/exe", &target)?;
    let mut permissions = std::fs::metadata(&target)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&target, permissions)?;
    Ok(())
}

pub trait ContainerHandle {
    fn wait(&mut self) -> Result<u32>;
}

impl ContainerHandle for std::process::Child {
    fn wait(&mut self) -> Result<u32> {
        Ok(std::process::Child::wait(self)?
            .code()
            .unwrap_or(0)
            .try_into()
            .unwrap())
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        if self.keep {
            tracing::warn!(
                "Keeping container at {}, not cleaning up",
                self.root.display()
            );
            return;
        }

        for mount in &self.mounts {
            let target_dir = self.root.join(mount);
            if let Err(e) = unmount(&target_dir) {
                tracing::error!(
                    "Failed cleaning up bind mount {}: {e}",
                    target_dir.display()
                );
            }
        }

        if let Err(e) = std::fs::remove_dir_all(&self.root) {
            tracing::error!(
                "Failed cleaning up container at {}: {e}",
                self.root.display()
            );
        }
    }
}
