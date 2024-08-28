use anyhow::Result;
use std::{collections::HashSet, path::Path};

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

pub fn mount_proc(target_dir: impl AsRef<Path>) -> Result<()> {
    use nix::mount::MsFlags;

    let target_dir = target_dir.as_ref();
    nix::mount::mount(
        Some("proc"),
        target_dir,
        Some("proc"),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        Option::<&str>::None,
    )?;
    Ok(())
}

pub fn unmount(path: impl AsRef<Path>) -> Result<()> {
    nix::mount::umount(path.as_ref())?;
    Ok(())
}

pub fn str_as_array<const T: usize>(v: impl AsRef<str>) -> [u8; T] {
    let mut arr = [0; T];
    let by = v.as_ref().as_bytes();
    arr[..by.len().min(T - 1)].copy_from_slice(&by[..by.len().min(T - 1)]);
    arr
}
