use anyhow::{Context, Result};
use derive_builder::Builder;
use derive_more::derive::{Deref, DerefMut};
use nix::unistd::{Gid, Pid, Uid};
use tracing::{error, info, instrument, trace, warn, Level};

use std::{
    ffi::{CString, OsStr},
    ops::Deref,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command},
};

use crate::{
    cli_wrappers::slirp::Slirp,
    command::ChildProcess,
    env::EnvVariable,
    host_tools::get_host_tools,
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
    tempdir: tempdir::TempDir,
    root: PathBuf,
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
        let tempdir = tempdir::TempDir::new("containix-container").context("Creating tempdir")?;
        let root = tempdir.path().join("root");
        std::fs::create_dir_all(&root)
            .with_context(|| format!("Creating rootfs at {}", root.display()))?;

        if container.rootfs.is_some() {
            warn!("Not sure how rootfs got set, but it isn’t supported yet.");
        }

        let nix_mounts = container
            .nix_components
            .into_iter()
            .map(|item| {
                let target = root.join(item.rootless());
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
                let dest = root.join(volume_mount.container_path.rootless());
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
            tempdir,
            root,
        })
    }
}

impl ContainerFsGuard {
    fn tmpdir(&self) -> &Path {
        self.tempdir.path()
    }
}

impl Deref for ContainerFsGuard {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

impl AsRef<Path> for ContainerFsGuard {
    fn as_ref(&self) -> &Path {
        self
    }
}

#[derive(Debug, Builder)]
#[builder(pattern = "owned")]
#[builder(build_fn(name = __build, vis = ""))]
pub struct Container {
    root: ContainerFsGuard,
    // #[builder(default, setter(strip_option, into))]
    // uid: Option<u32>,
    // #[builder(default, setter(strip_option, into))]
    // gid: Option<u32>,
    #[builder(default, setter(custom, name = "env"))]
    envs: Vec<EnvVariable>,
    #[builder(setter(into))]
    command: String,
    #[builder(default, setter(custom, name = "arg"))]
    args: Vec<String>,
}

#[allow(dead_code)]
impl ContainerBuilder {
    pub fn env(self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.envs([EnvVariable::new(key, value)])
    }

    pub fn envs(mut self, envs: impl IntoIterator<Item = EnvVariable>) -> Self {
        self.envs
            .get_or_insert_with(std::vec::Vec::new)
            .extend(envs);
        self
    }

    pub fn arg(mut self, arg: impl AsRef<str>) -> Self {
        self.args
            .get_or_insert_with(std::vec::Vec::new)
            .push(arg.as_ref().to_string());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.args
            .get_or_insert_with(std::vec::Vec::new)
            .extend(args.into_iter().map(|v| v.as_ref().to_string()));
        self
    }

    #[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
    pub fn spawn<'a>(self) -> Result<ContainerGuard<impl ChildProcess, impl ChildProcess>> {
        let opts = self.__build()?;
        let mut unshare_builder = UnshareEnvironmentBuilder::default();
        unshare_builder
            .namespace(UnshareNamespaces::Mount)
            .namespace(UnshareNamespaces::Pid)
            .namespace(UnshareNamespaces::Ipc)
            .namespace(UnshareNamespaces::User)
            .namespace(UnshareNamespaces::Uts)
            .namespace(UnshareNamespaces::Network)
            .map_current_user_to_root()
            .root(opts.root.as_ref());

        let handle = unshare_builder
            .execute(move || {
                let mut cmd = Command::new(&opts.command);
                cmd.args(&opts.args).env_clear().envs(
                    opts.envs
                        .iter()
                        .map(|v| (v.key.as_os_str(), v.value.as_os_str())),
                );
                let err = cmd.exec();
                error!("Failed to exec: {err}");
                -100
            })
            .context("Entering unshare environment")?;

        let slirp = Slirp::default()
            .binary(get_host_tools().join("bin").join("slirp4netns"))
            .pid(handle.pid())
            .socket(opts.root.tempdir.path().join("slirp.sock"))
            .activate()
            .context("Activating slirp")?;

        return Ok(ContainerGuard {
            slirp,
            handle,
            root: opts.root,
        });
    }
}

#[derive(Debug, Deref, DerefMut)]
pub struct ContainerGuard<T: ChildProcess, T2: ChildProcess> {
    slirp: T2,
    #[deref]
    #[deref_mut]
    handle: T,
    root: ContainerFsGuard,
}

impl<T: ChildProcess, T2: ChildProcess> AsRef<Path> for ContainerGuard<T, T2> {
    fn as_ref(&self) -> &Path {
        self.root.as_ref()
    }
}

impl<T: ChildProcess, T2: ChildProcess> ContainerGuard<T, T2> {
    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }
}

impl<T: ChildProcess, T2: ChildProcess> Drop for ContainerGuard<T, T2> {
    fn drop(&mut self) {
        if let Err(e) = self.slirp.kill() {
            error!("Failed to kill slirp: {e}");
        }
    }
}
