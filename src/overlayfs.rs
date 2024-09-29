use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{bail, Context, Result};
use derive_more::derive::Deref;
use tracing::{debug, error, instrument, trace};
use typed_builder::TypedBuilder;

use crate::{command::run_command, tools::TOOLS};

static MOUNT: LazyLock<OsString> = LazyLock::new(|| TOOLS.get("mount").unwrap().path.clone());
static UMOUNT: LazyLock<OsString> = LazyLock::new(|| TOOLS.get("umount").unwrap().path.clone());

#[instrument(level = "trace", skip_all, fields(ty = ?ty.as_ref().map(|v| v.as_ref()), src = %src.as_ref().display(), target = %target.as_ref().display()))]
pub fn mount(
    ty: Option<impl AsRef<OsStr>>,
    src: impl AsRef<Path>,
    target: impl AsRef<Path>,
    opts: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<MountGuard> {
    let mut cmd = std::process::Command::new(MOUNT.as_os_str());
    if let Some(ty) = ty {
        cmd.arg("-t").arg(ty.as_ref());
    }
    cmd.arg(src.as_ref());

    for opt in opts {
        cmd.arg("-o");
        cmd.arg(opt.as_ref());
    }

    let target = target.as_ref().to_path_buf();
    cmd.arg(&target);

    let output = run_command(cmd)?;
    if !output.status.success() {
        error!(
            "Failed to mount {}: {}",
            target.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        bail!("Failed to mount {}", target.display());
    }
    Ok(MountGuard(target))
}

#[derive(Debug, Deref, PartialEq)]
pub struct MountGuard(PathBuf);
impl Drop for MountGuard {
    fn drop(&mut self) {
        let mut cmd = std::process::Command::new(UMOUNT.as_os_str());
        cmd.arg(&self.0);
        let Ok(output) = cmd.output() else {
            error!("Failed to run unmount on {}", self.0.display());
            return;
        };
        if !output.status.success() {
            error!(
                "Failed to unmount {}: {}",
                self.0.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
