use anyhow::{Context, Result};
use derive_builder::Builder;
use tracing::{instrument, warn, Level};

use std::{
    ffi::{CString, OsStr},
    ops::Deref,
    path::{Path, PathBuf},
};

use crate::{
    mount::{BindMount, MountGuard},
    path_ext::PathExt,
    unshare::{UnshareEnvironmentBuilder, UnshareNamespaces},
    volume_mount::VolumeMount,
};

#[derive(Debug, Clone, Builder)]
#[builder(build_fn(name = __build, vis = ""))]
pub struct ContainerFs {
    #[builder(default, setter(into, strip_option))]
    rootfs: Option<PathBuf>,
    #[builder(default, setter(custom, name = "volume"))]
    volumes: Vec<VolumeMount>,
    #[builder(default, setter(custom, name = "nix_component"))]
    nix_components: Vec<PathBuf>,
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
        self.volumes
            .get_or_insert_with(std::vec::Vec::new)
            .push(volume_mount);
        self
    }

    pub fn nix_component(&mut self, nix_mount: impl AsRef<Path>) -> &mut Self {
        self.nix_components
            .get_or_insert_with(std::vec::Vec::new)
            .push(nix_mount.as_ref().to_path_buf());
        self
    }

    #[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
    pub fn build(self) -> Result<ContainerFsGuard> {
        let container = self.__build()?;
        let root = tempdir::TempDir::new("containix-container").context("Creating tempdir")?;

        if container.rootfs.is_some() {
            warn!("Not sure how rootfs got set, but it isn’t supported yet.");
        }

        let nix_mounts = container
            .nix_components
            .into_iter()
            .map(|item| {
                let target = root.path().join(item.rootless());
                std::fs::create_dir_all(&target)?;
                BindMount::default()
                    .src(&item)
                    .dest(&target)
                    .read_only(true)
                    .cleanup(false)
                    .mount()
                    .with_context(|| format!("Mounting {}", item.display()))
            })
            .collect::<Result<Vec<_>>>()?;

        let volume_mounts = container
            .volumes
            .into_iter()
            .map(|volume_mount| {
                let src = volume_mount.host_path.as_path();
                let dest = root.path().join(volume_mount.container_path.rootless());
                std::fs::create_dir_all(&dest)
                    .with_context(|| format!("Creating directory {dest:?} for volume mount"))?;
                BindMount::default()
                    .src(src)
                    .dest(&dest)
                    .read_only(volume_mount.read_only)
                    .cleanup(false)
                    .mount()
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
        self
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

    #[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
    pub fn spawn(
        &self,
        command: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        path_var: impl AsRef<OsStr>,
    ) -> Result<impl ContainerHandle> {
        let mut unshare_builder = UnshareEnvironmentBuilder::default();
        unshare_builder
            .namespace(UnshareNamespaces::Mount)
            .namespace(UnshareNamespaces::Pid)
            .namespace(UnshareNamespaces::Ipc)
            .namespace(UnshareNamespaces::User)
            .namespace(UnshareNamespaces::Uts)
            .map_current_user_to_root()
            .root(self.root())
            .fork(true);

        // .namespace(UnshareNamespaces::Network)

        match unshare_builder
            .enter()
            .context("Entering unshare environment")?
        {
            None => {
                nix::unistd::execvpe(
                    CString::new(command.as_ref().as_encoded_bytes())?.as_c_str(),
                    &args
                        .into_iter()
                        .map(|s| Ok(CString::new(s.as_ref().as_encoded_bytes())?))
                        .collect::<Result<Vec<_>>>()?,
                    // FIXME: Should probably avoid lossy function here
                    &[CString::new(format!(
                        "PATH={}",
                        path_var.as_ref().to_string_lossy()
                    ))?],
                )?;
                unreachable!()
            }
            Some(handle) => Ok(handle),
        }
    }
}

#[allow(dead_code)]
pub trait ContainerHandle {
    /// Get the PID of the container.
    fn pid(&self) -> u32;
    /// Wait for the container to exit and return the exit code.
    fn wait(&mut self) -> Result<i32>;
}
