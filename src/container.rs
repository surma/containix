use anyhow::{Context, Result};
use derive_builder::Builder;
use tracing::{info, instrument, trace, warn};

use std::{
    ffi::{OsStr, OsString},
    ops::Deref,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::{
    mount::{bind_mount, MountGuard},
    path_ext::PathExt,
    tools::TOOLS,
    volume_mount::VolumeMount,
};

static UNSHARE: LazyLock<OsString> = LazyLock::new(|| TOOLS.get("unshare").unwrap().path.clone());

#[derive(Debug, Clone, Builder)]
#[builder(build_fn(name = __build, vis = ""))]
pub struct ContainerFs {
    #[builder(default, setter(into, strip_option))]
    rootfs: Option<PathBuf>,
    #[builder(default, setter(custom))]
    volume: Vec<VolumeMount>,
    #[builder(default, setter(custom))]
    nix_component: Vec<PathBuf>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ContainerFsGuard {
    // Order is important here, as drop runs in order of declaration.
    // https://doc.rust-lang.org/stable/std/ops/trait.Drop.html#drop-order
    volume_mounts: Vec<MountGuard>,
    nix_mounts: Vec<MountGuard>,
    root: tempdir::TempDir,
}

impl ContainerFsBuilder {
    pub fn volume(&mut self, volume_mount: VolumeMount) -> &mut Self {
        self.volume.get_or_insert_with(|| vec![]).push(volume_mount);
        self
    }

    pub fn nix_component(&mut self, nix_mount: impl AsRef<Path>) -> &mut Self {
        self.nix_component
            .get_or_insert_with(|| vec![])
            .push(nix_mount.as_ref().to_path_buf());
        self
    }

    pub fn build(self) -> Result<ContainerFsGuard> {
        let container = self.__build()?;
        let root = tempdir::TempDir::new("containix-container").context("Creating tempdir")?;

        if let Some(_) = container.rootfs {
            warn!("Not sure how rootfs got set, but it isn’t supported yet.");
        }

        let nix_mounts = container
            .nix_component
            .into_iter()
            .map(|item| {
                let target = root.path().join(item.rootless());
                std::fs::create_dir_all(&target)?;
                bind_mount(&item, &target, true)
                    .with_context(|| format!("Mounting {}", item.display()))
            })
            .collect::<Result<Vec<_>>>()?;

        let volume_mounts = container
            .volume
            .into_iter()
            .map(|volume_mount| {
                let src = volume_mount.host_path.as_path();
                let dest = root.path().join(volume_mount.container_path.rootless());
                std::fs::create_dir_all(&dest)
                    .with_context(|| format!("Creating directory {dest:?} for volume mount"))?;
                bind_mount(&src, &dest, volume_mount.read_only)
                    .with_context(|| format!("Mounting {src:?} -> {dest:?}"))
            })
            .collect::<Result<Vec<_>>>()
            .context("Mounting volumes")?;

        Ok(ContainerFsGuard {
            volume_mounts,
            nix_mounts,
            root,
        })
    }
}

impl Deref for ContainerFsGuard {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.root.path()
    }
}

impl AsRef<Path> for ContainerFsGuard {
    fn as_ref(&self) -> &Path {
        &*self
    }
}

#[derive(Debug)]
pub struct UnshareContainer<T: AsRef<Path>> {
    root: T,
    keep: bool,
}

impl<T: AsRef<Path>> UnshareContainer<T> {
    pub fn new(root: T) -> Result<Self> {
        Ok(Self { root, keep: false })
    }

    pub fn set_keep(&mut self, keep: bool) {
        self.keep = keep;
    }

    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }

    pub fn spawn(
        &self,
        command: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        path_var: impl AsRef<OsStr>,
    ) -> Result<impl ContainerHandle> {
        let mut unshare = std::process::Command::new(&*UNSHARE);
        unshare.arg("--root");
        unshare.arg(self.root());
        unshare.arg("--fork");
        unshare.arg("--mount");
        unshare.arg("--pid");
        unshare.arg("--ipc");
        unshare.arg("--map-root-user");
        unshare.arg(command.as_ref());
        unshare.args(args);
        unshare.env_clear();
        unshare.env("CONTAINIX_CONTAINER", "1");
        unshare.env("PATH", path_var.as_ref());
        if let Ok(log) = std::env::var("RUST_LOG") {
            unshare.env("RUST_LOG", log);
        }
        unshare.stdout(std::process::Stdio::inherit());
        unshare.stderr(std::process::Stdio::inherit());
        unshare.stdin(std::process::Stdio::inherit());
        tracing::trace!(
            "Running {} {:?}",
            unshare.get_program().to_string_lossy(),
            unshare.get_args().collect::<Vec<_>>()
        );
        let child = unshare.spawn()?;
        Ok(child)
    }
}

pub trait ContainerHandle {
    fn pid(&self) -> u32;
    fn wait(&mut self) -> Result<u32>;
}

impl ContainerHandle for std::process::Child {
    fn pid(&self) -> u32 {
        std::process::Child::id(self)
    }
    fn wait(&mut self) -> Result<u32> {
        Ok(std::process::Child::wait(self)?
            .code()
            .unwrap_or(0)
            .try_into()
            .unwrap())
    }
}
