use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn bind_mount(
    src: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
    read_only: bool,
) -> Result<()> {
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
    Ok(())
}

pub fn unmount(path: impl AsRef<Path>) -> Result<()> {
    nix::mount::umount(path.as_ref())?;
    Ok(())
}
