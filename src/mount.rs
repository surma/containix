use anyhow::Result;
use derive_builder::Builder;
use derive_more::derive::Deref;
use std::path::{Path, PathBuf};
use tracing::{error, instrument, trace};

#[derive(Debug, Deref, PartialEq)]
pub struct MountGuard(Option<PathBuf>);
impl Drop for MountGuard {
    fn drop(&mut self) {
        let Some(path) = &self.0 else {
            return;
        };
        if let Err(err) = unmount(&path) {
            error!("Failed to unmount {}: {}", path.display(), err);
        }
    }
}

#[derive(Debug, Clone, Builder)]
#[builder(name = "BindMount", setter(into))]
#[builder(build_fn(vis = ""))]
pub struct BindMountOptions {
    src: PathBuf,
    dest: PathBuf,
    #[builder(default)]
    read_only: bool,
    #[builder(default = "true")]
    cleanup: bool,
}

impl BindMount {
    #[instrument(level = "trace", skip_all, err(level = "trace"))]
    pub fn mount(&mut self) -> Result<MountGuard> {
        let opts = self.build()?;
        trace!("Mounting {opts:?}");
        use nix::mount::MsFlags;

        nix::mount::mount(
            Some(&opts.src),
            &opts.dest,
            Option::<&str>::None,
            MsFlags::MS_BIND.union(if opts.read_only {
                MsFlags::MS_RDONLY
            } else {
                MsFlags::empty()
            }),
            Option::<&str>::None,
        )?;
        Ok(MountGuard(if opts.cleanup {
            Some(opts.dest)
        } else {
            None
        }))
    }
}

// #[instrument(level = "trace", skip_all, fields(src = %src.as_ref().display(), target_dir = %target_dir.as_ref().display(), read_only = %read_only), err(level = "trace"))]
// pub fn bind_mount(
//     src: impl AsRef<Path>,
//     target_dir: impl AsRef<Path>,
//     read_only: bool,
// ) -> Result<MountGuard> {
// }

#[instrument(level = "trace", skip_all, fields(path = %path.as_ref().display()), err(level = "trace"))]
pub fn unmount(path: impl AsRef<Path>) -> Result<()> {
    nix::mount::umount(path.as_ref())?;
    Ok(())
}
