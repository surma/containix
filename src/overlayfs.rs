use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use anyhow::{bail, Result};
use derive_more::derive::{Deref, DerefMut};
use tracing::error;

#[derive(Debug, Clone, PartialEq, Deref, DerefMut)]
pub struct OverlayFs(PathBuf);
impl OverlayFs {
    pub fn new(
        lower: Vec<PathBuf>,
        upper: Option<PathBuf>,
        work: Option<PathBuf>,
        target: PathBuf,
    ) -> Result<Self> {
        let mut cmd = std::process::Command::new("mount");
        cmd.arg("-t").arg("overlay").arg("overlay");

        let mut lower_opt = OsString::from("-olowerdir=");
        lower_opt.push(
            lower
                .iter()
                .map(|p| p.as_os_str())
                .collect::<Vec<_>>()
                .join(OsStr::new(":")),
        );
        cmd.arg(lower_opt);

        if let Some(upper) = upper {
            std::fs::create_dir_all(&upper)?;

            let mut upper_opt = OsString::from("-oupperdir=");
            upper_opt.push(upper);
            cmd.arg(upper_opt);
        }

        if let Some(work) = work {
            std::fs::create_dir_all(&work)?;
            let mut work_opt = OsString::from("-oworkdir=");
            work_opt.push(work);
            cmd.arg(work_opt);
        }

        cmd.arg(&target);

        let output = cmd.output()?;
        if !output.status.success() {
            error!(
                "Failed to mount overlayfs: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            bail!("Failed to mount overlayfs");
        }
        Ok(OverlayFs(target))
    }
}

impl Drop for OverlayFs {
    fn drop(&mut self) {
        fn inner(fs: &OverlayFs) -> Result<()> {
            use anyhow::Context;

            let mut cmd = std::process::Command::new("umount");
            cmd.arg(&fs.0);
            let output = cmd.output().context("Failed to run `unmount`")?;
            if !output.status.success() {
                error!(
                    "Failed to unmount overlayfs: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                bail!("Failed to unmount overlayfs");
            }
            Ok(())
        }
        match inner(self) {
            Ok(_) => (),
            Err(e) => error!("Failed to cleanup overlayfs {}: {e}", self.0.display()),
        }
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
            let overlayfs = OverlayFs::new(
                vec![tmpdir.path().join("lower_1"), tmpdir.path().join("lower_2")],
                None,
                None,
                target.clone(),
            )?;
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
        let overlayfs = OverlayFs::new(
            vec![tmpdir.path().join("lower_2"), tmpdir.path().join("lower_1")],
            None,
            None,
            target,
        )?;

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
        let overlayfs = OverlayFs::new(
            vec![tmpdir.path().join("lower")],
            Some(tmpdir.path().join("upper")),
            Some(tmpdir.path().join("work")),
            target.clone(),
        )?;
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
        let overlayfs = OverlayFs::new(
            vec![tmpdir.path().join("lower")],
            Some(tmpdir.path().join("upper")),
            Some(tmpdir.path().join("work")),
            target.clone(),
        )?;
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
