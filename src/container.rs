use anyhow::{Context, Result};
use derive_more::derive::{Deref, DerefMut};
use tracing::{instrument, trace};
use typed_builder::TypedBuilder;

use std::{
    ffi::{OsStr, OsString},
    mem::ManuallyDrop,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};

use crate::{
    command_wrappers::{bind_mount, unmount},
    nix_helpers::NixStoreItem,
    overlayfs::{mount, MountGuard, OverlayFs, OverlayFsGuard},
    tools::TOOLS,
};

static UNSHARE: LazyLock<OsString> = LazyLock::new(|| TOOLS.get("unshare").unwrap().path.clone());

#[derive(Debug, Clone, TypedBuilder)]
#[builder(builder_method(name = build))]
#[builder(build_method(name = __create, vis = ""))]
#[builder(mutators(
    pub fn add_volume_mount(&mut self, src: impl Into<PathBuf>, target: impl Into<PathBuf>) {
        self.volumes.push((src.into(), target.into()));
    }
))]
#[builder(mutators(
    pub fn expose_nix_item(&mut self, item: impl Into<PathBuf>) {
        self.nix_mounts.push(item.into());
    }
))]
pub struct ContainerFs {
    #[builder(setter(into))]
    rootfs: PathBuf,
    #[builder(via_mutators(init = Vec::new()))]
    volumes: Vec<(PathBuf, PathBuf)>,
    #[builder(via_mutators(init = Vec::new()))]
    nix_mounts: Vec<PathBuf>,
}

#[derive(Debug, Deref)]
pub struct ContainerFsGuard {
    // Order is important here, as drop runs in order of declaration.
    // https://doc.rust-lang.org/stable/std/ops/trait.Drop.html#drop-order
    volumes: Vec<MountGuard>,
    nix_mounts: Vec<MountGuard>,
    #[deref]
    rootfs: OverlayFsGuard,
    base: tempdir::TempDir,
}

#[allow(dead_code, non_camel_case_types, missing_docs)]
#[automatically_derived]
impl ContainerFsBuilder<((PathBuf,), (Vec<(PathBuf, PathBuf)>,), (Vec<PathBuf>,))> {
    #[instrument(level = "trace", skip_all)]
    #[allow(
        clippy::default_trait_access,
        clippy::used_underscore_binding,
        clippy::no_effect_underscore_binding
    )]
    pub fn create(self) -> Result<ContainerFsGuard> {
        let container = self.__create();
        let base = tempdir::TempDir::new("containix-container").context("Creating tempdir")?;
        std::fs::create_dir_all(base.path().join("root")).context("Creating root")?;

        let misc = base.path().join("misc");
        std::fs::create_dir_all(misc.join("proc")).context("Creating proc dir")?;

        let upper_dir = base.path().join("upper");
        let work_dir = base.path().join("work");
        let rootfs = OverlayFs::builder()
            .add_lower(container.rootfs)
            .add_lower(misc)
            .upper(upper_dir)
            .work(work_dir)
            .mount(base.path().join("root"))?;

        let nix_mounts = container
            .nix_mounts
            .into_iter()
            .map(|item| {
                let target = rootfs.join(item.strip_prefix("/").unwrap_or(&item));
                std::fs::create_dir_all(&target)?;
                mount(Option::<&str>::None, &item, &target, ["bind,ro"])
            })
            .collect::<Result<Vec<_>>>()?;

        let volumes = container
            .volumes
            .into_iter()
            .map(|(src, target)| {
                let target_dir = rootfs.join(target);
                std::fs::create_dir_all(&target_dir)
                    .context("Creating directory for bind mount")?;
                mount(Option::<&str>::None, &src, &target_dir, ["bind,ro"])
                    .with_context(|| format!("Mounting {src:?} -> {target_dir:?}"))
            })
            .collect::<Result<Vec<_>>>()
            .context("Mounting volumes")?;

        Ok(ContainerFsGuard {
            volumes,
            rootfs,
            nix_mounts,
            base,
        })
    }
}

impl AsRef<Path> for ContainerFsGuard {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

#[derive(Debug)]
pub struct UnshareContainer<T: AsRef<Path>> {
    root: T,
    keep: bool,
    netns: bool,
}

impl<T: AsRef<Path>> UnshareContainer<T> {
    pub fn new(root: T) -> Result<Self> {
        Ok(Self {
            root,
            keep: false,
            netns: false,
        })
    }

    pub fn set_keep(&mut self, keep: bool) {
        self.keep = keep;
    }

    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }

    pub fn set_netns(&mut self, netns: bool) {
        self.netns = netns;
    }

    pub fn spawn(
        &self,
        command: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        path_var: impl AsRef<OsStr>,
    ) -> Result<impl ContainerHandle> {
        std::fs::create_dir_all(self.root().join("proc")).context("Creating proc directory")?;

        let mut unshare = std::process::Command::new(&*UNSHARE);
        unshare.arg("--root");
        unshare.arg(self.root());
        unshare.arg("--fork");
        unshare.arg("--mount");
        unshare.arg("--pid");
        unshare.arg("--ipc");
        if self.netns {
            unshare.arg("--net");
        }
        unshare.arg("--mount-proc=/proc");
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
