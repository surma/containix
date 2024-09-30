use anyhow::Result;
use derive_more::derive::Deref;
use std::path::{Path, PathBuf};
use tracing::{error, instrument};

#[derive(Debug, Deref, PartialEq)]
pub struct MountGuard(PathBuf);
impl Drop for MountGuard {
    fn drop(&mut self) {
        if let Err(err) = unmount(&self.0) {
            error!("Failed to unmount {}: {}", self.0.display(), err);
        }
    }
}

#[instrument(level = "trace", skip_all, fields(src = %src.as_ref().display(), target_dir = %target_dir.as_ref().display(), read_only = %read_only), err(level = "trace"))]
pub fn bind_mount(
    src: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
    read_only: bool,
) -> Result<MountGuard> {
    use nix::mount::MsFlags;

    let src = src.as_ref();
    let target_dir = target_dir.as_ref();
    nix::mount::mount(
        Some(src),
        target_dir,
        Option::<&str>::None,
        MsFlags::MS_BIND
            | (if read_only {
                MsFlags::MS_RDONLY
            } else {
                MsFlags::empty()
            }),
        Option::<&str>::None,
    )?;
    Ok(MountGuard(target_dir.into()))
}

#[instrument(level = "trace", skip_all, fields(path = %path.as_ref().display()), err(level = "trace"))]
pub fn unmount(path: impl AsRef<Path>) -> Result<()> {
    nix::mount::umount(path.as_ref())?;
    Ok(())
}
