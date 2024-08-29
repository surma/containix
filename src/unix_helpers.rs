use anyhow::{Context, Result};
use std::path::Path;

use crate::{command::run_command, tools::UTIL_COMPONENT};

pub fn bind_mount(src: impl AsRef<Path>, dst: impl AsRef<Path>, read_only: bool) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    let mut command = std::process::Command::new(UTIL_COMPONENT.join("bin").join("mount"));
    command.arg("-o");
    if read_only {
        command.arg("bind,ro");
    } else {
        command.arg("bind");
    }
    command.arg(src);
    command.arg(dst);
    run_command(command).context("Running mount")?;

    Ok(())
}

pub fn unmount(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let mut command = std::process::Command::new(UTIL_COMPONENT.join("bin").join("umount"));
    command.arg(path);
    run_command(command).context("Running unmount")?;
    Ok(())
}
