use anyhow::{Context, Result};
use derive_more::derive::Deref;

use std::{
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};

use crate::unix_helpers::{bind_mount, mount_proc, unmount};

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

    fn mount_proc(&self) -> Result<()> {
        let proc = Path::new("/proc");
        std::fs::create_dir_all(proc).context("Creating proc directory")?;
        mount_proc(proc).context("Mounting proc")?;
        Ok(())
    }

    pub fn spawn(&self, mut command: Command) -> Result<ContainerHandle> {
        use nix::sched::CloneFlags;

        nix::sched::unshare(
            CloneFlags::CLONE_NEWNS
                | CloneFlags::CLONE_NEWPID
                | CloneFlags::CLONE_FILES
                | CloneFlags::CLONE_FS,
            // | CloneFlags::CLONE_NEWNET,
        )
        .context("Unsharing namespaces")?;

        match unsafe { nix::unistd::fork() }.context("Forking")? {
            nix::unistd::ForkResult::Child => {
                nix::unistd::chroot(self.root()).context("Chrooting container")?;
                self.mount_proc().context("Mounting proc in container")?;
                Result::Err(command.exec()).context("Executing command in container")
            }
            nix::unistd::ForkResult::Parent { child } => Ok(ContainerHandle(child)),
        }
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

#[derive(Debug, Deref, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct ContainerHandle(nix::unistd::Pid);
impl ContainerHandle {
    pub fn wait(&self) -> Result<nix::sys::wait::WaitStatus> {
        Ok(nix::sys::wait::waitpid(self.0, None)?)
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

        let proc_dir = self.root.join("proc");
        if let Ok(metadata) = std::fs::metadata(&proc_dir) {
            if metadata.is_dir() {
                if let Err(e) = unmount(&proc_dir) {
                    tracing::error!("Failed unmounting proc: {e}");
                }
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
