use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{bail, Context, Result};
use derive_more::derive::{Deref, DerefMut};
use tracing::{debug, error, instrument, trace};
use typed_builder::TypedBuilder;

use crate::tools::TOOLS;

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
    debug!("Running mount command: {:?}", cmd);

    let output = cmd.output()?;
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

#[derive(Debug, Clone, TypedBuilder)]
#[builder(mutators(
    pub fn add_lower(&mut self, path: impl Into<PathBuf>) {
        self.lower.push(path.into());
    }
))]
#[builder(build_method(name = __finish, vis = ""))]
pub struct OverlayFs {
    #[builder(via_mutators(init = Vec::new()))]
    lower: Vec<PathBuf>,
    #[builder(default, setter(strip_option, into))]
    upper: Option<PathBuf>,
    #[builder(default, setter(strip_option, into))]
    work: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Deref)]
pub struct OverlayFsGuard(MountGuard);
// Scary types here taken from `cargo expand`.
#[allow(dead_code, non_camel_case_types, missing_docs)]
impl<
        __upper: ::typed_builder::Optional<Option<PathBuf>>,
        __work: ::typed_builder::Optional<Option<PathBuf>>,
    > OverlayFsBuilder<((Vec<PathBuf>,), __upper, __work)>
{
    #[instrument(level = "trace", skip_all, fields(target = %target.as_ref().display()), ret)]
    #[allow(
        clippy::default_trait_access,
        clippy::used_underscore_binding,
        clippy::no_effect_underscore_binding
    )]
    pub fn mount(self, target: impl AsRef<Path>) -> Result<OverlayFsGuard> {
        let ofs = self.__finish();
        trace!("OverlayFs: {:?}", ofs);
        let mut opts = vec![];

        let mut lower_opt = OsString::from("lowerdir=");
        lower_opt.push(
            ofs.lower
                .iter()
                .rev()
                .map(|p| p.as_os_str())
                .collect::<Vec<_>>()
                .join(OsStr::new(":")),
        );
        opts.push(lower_opt);

        if let Some(upper) = &ofs.upper {
            std::fs::create_dir_all(&upper).context("Creating upper directory")?;

            let mut upper_opt = OsString::from("upperdir=");
            upper_opt.push(upper);
            opts.push(upper_opt);
        }

        if let Some(work) = &ofs.work {
            std::fs::create_dir_all(&work).context("Creating work directory")?;
            let mut work_opt = OsString::from("workdir=");
            work_opt.push(work);
            opts.push(work_opt);
        }

        let guard =
            mount(Some("overlay"), "overlay", target, opts).context("Mounting overlayfs")?;
        Ok(OverlayFsGuard(guard))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    use anyhow::{Context, Result};

    fn create_tmp_folder_structure(
        items: impl IntoIterator<Item = (impl AsRef<Path>, impl AsRef<str>)>,
    ) -> Result<tempdir::TempDir> {
        use ::anyhow::Context;

        let tmpdir = tempdir::TempDir::new("overlayfs-test")?;
        std::fs::create_dir_all(tmpdir.path())
            .with_context(|| format!("Creating tempdir {}", tmpdir.path().display()))?;
        for (path, content) in items {
            let path = tmpdir.path().join(path.as_ref());
            let dir = path
                .parent()
                .with_context(|| format!("Getting parent of {}", path.display()))?;
            std::fs::create_dir_all(dir)
                .with_context(|| format!("Creating directory {}", dir.display()))?;
            std::fs::write(&path, content.as_ref())
                .with_context(|| format!("Writing to {}", path.display()))?;
        }
        Ok(tmpdir)
    }

    #[test]
    #[cfg_attr(not(target_os = "linux"), ignore = "overlayfs is a Linux feature")]
    fn test_readonly_simple() -> Result<()> {
        let tmpdir = create_tmp_folder_structure(vec![
            ("lower_1/a", "a"),
            ("lower_2/b", "b"),
            ("target/.empty", ""),
        ])?;

        let target = tmpdir.path().join("target");
        {
            let mut overlayfs = OverlayFs::builder()
                .add_lower(tmpdir.path().join("lower_1"))
                .add_lower(tmpdir.path().join("lower_2"))
                .mount(target.clone())?;
            assert!(std::fs::metadata(&overlayfs.join("a"))?.is_file());
            assert!(std::fs::metadata(&overlayfs.join("a"))?.is_file());
            assert!(std::fs::metadata(&overlayfs.join("b"))?.is_file());
            assert!(std::fs::write(overlayfs.join("c"), "a_new").is_err());
        }
        assert!(std::fs::metadata(&target.join("a"))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound));
        assert!(std::fs::metadata(&target.join("b"))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound));
        Ok(())
    }

    #[test]
    #[cfg_attr(not(target_os = "linux"), ignore = "overlayfs is a Linux feature")]
    fn test_shadow_order() -> Result<()> {
        let tmpdir = create_tmp_folder_structure(vec![
            ("lower_1/f/a", "1"),
            ("lower_1/f/b", "1"),
            ("lower_1/c", "1"),
            ("lower_2/f/a", "2"),
            ("lower_2/f/d", "2"),
            ("lower_2/c", "2"),
            ("lower_2/e", "2"),
            ("target/.empty", ""),
        ])?;

        let target = tmpdir.path().join("target");
        let mut overlayfs = OverlayFs::builder()
            .add_lower(tmpdir.path().join("lower_1"))
            .add_lower(tmpdir.path().join("lower_2"))
            .mount(target.clone())?;

        assert_eq!(std::fs::read_to_string(overlayfs.join("f/a"))?, "2");
        assert_eq!(std::fs::read_to_string(overlayfs.join("f/b"))?, "1");
        assert_eq!(std::fs::read_to_string(overlayfs.join("f/d"))?, "2");
        assert_eq!(std::fs::read_to_string(overlayfs.join("c"))?, "2");
        assert_eq!(std::fs::read_to_string(overlayfs.join("e"))?, "2");
        Ok(())
    }

    #[test]
    #[cfg_attr(not(target_os = "linux"), ignore = "overlayfs is a Linux feature")]
    fn test_write() -> Result<()> {
        let tmpdir = create_tmp_folder_structure(vec![
            ("lower/f/a", "1"),
            ("lower/f/b", "1"),
            ("lower/c", "1"),
            ("upper/f/a", "2"),
            ("work/.empty", ""),
            ("target/.empty", ""),
        ])?;

        let target = tmpdir.path().join("target");
        let mut overlayfs = OverlayFs::builder()
            .add_lower(tmpdir.path().join("lower"))
            .upper(tmpdir.path().join("upper"))
            .work(tmpdir.path().join("work"))
            .mount(target.clone())?;

        std::fs::write(overlayfs.join("d"), "lol").context("Writing to d")?;
        std::fs::remove_dir_all(overlayfs.join("f")).context("Deleting f")?;

        assert_eq!(
            std::fs::read_to_string(overlayfs.join("d")).context("Reading d")?,
            "lol"
        );
        assert!(std::fs::metadata(target.join("lower/d"))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound));

        assert!(std::fs::metadata(overlayfs.join("f"))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound));
        assert!(std::fs::metadata(tmpdir.path().join("lower/f"))
            .context("Stating lower/f")?
            .is_dir());
        Ok(())
    }

    #[test]
    #[cfg_attr(not(target_os = "linux"), ignore = "overlayfs is a Linux feature")]
    fn test_shadow_dir() -> Result<()> {
        let tmpdir = create_tmp_folder_structure(vec![
            ("lower/f/a", "1"),
            ("lower/f/b", "1"),
            ("upper/.empty", "2"),
            ("work/.empty", ""),
            ("target/.empty", ""),
        ])?;

        let target = tmpdir.path().join("target");
        let mut overlayfs = OverlayFs::builder()
            .add_lower(tmpdir.path().join("lower"))
            .upper(tmpdir.path().join("upper"))
            .work(tmpdir.path().join("work"))
            .mount(target.clone())?;

        std::fs::remove_dir_all(overlayfs.join("f")).context("Deleting f")?;
        std::fs::create_dir(overlayfs.join("f")).context("Creating f")?;
        std::fs::write(overlayfs.join("f/a"), "3").context("Writing f/a")?;

        assert_eq!(
            std::fs::read_to_string(overlayfs.join("f/a")).context("Reading f/a")?,
            "3"
        );
        assert!(std::fs::metadata(overlayfs.join("f/b"))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound));

        Ok(())
    }
}
