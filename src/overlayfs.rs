use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use derive_more::derive::{Deref, From};
use thiserror::Error;
use tracing::error;
use typed_builder::TypedBuilder;

#[derive(Debug, Error)]
pub enum OverlayFsError {
    #[error("Failed to create directory {0}: {1}")]
    FolderCreation(PathBuf, std::io::Error),
    #[error("Failed to mount overlayfs: {0}")]
    Mount(std::io::Error),
}

pub type Result<T> = std::result::Result<T, OverlayFsError>;

#[derive(Debug, Clone, PartialEq, TypedBuilder)]
#[builder(build_method(name = finish))]
pub struct OverlayFs {
    #[builder(default = vec![])]
    lower: Vec<PathBuf>,
    #[builder(setter(strip_option), default)]
    upper: Option<PathBuf>,
    #[builder(default = true)]
    create_upper_dir: bool,
    #[builder(default = true)]
    cleanup_upper_dir: bool,
    #[builder(default = "./work".into())]
    work: PathBuf,
    #[builder(default = true)]
    create_work_dir: bool,
    #[builder(default = true)]
    cleanup_work_dir: bool,
    target: PathBuf,
    #[builder(setter(strip_option), default)]
    base: Option<PathBuf>,
}

impl OverlayFs {
    pub fn work_dir(&self) -> PathBuf {
        self.base
            .clone()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(&self.work)
    }
    pub fn upper_dir(&self) -> Option<PathBuf> {
        Some(
            self.base
                .clone()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(self.upper.as_ref()?),
        )
    }

    pub fn target_dir(&self) -> PathBuf {
        self.base
            .clone()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(&self.target)
    }

    fn create_dir(&self, path: &PathBuf) -> Result<()> {
        std::fs::create_dir_all(path).map_err(|e| OverlayFsError::FolderCreation(path.clone(), e))
    }

    pub fn apply(self) -> Result<OverlayFsGuard> {
        if self.create_upper_dir {
            if let Some(upper_dir) = &self.upper_dir() {
                self.create_dir(upper_dir)?;
            }
        }
        if self.create_work_dir {
            self.create_dir(&self.work_dir())?;
        }

        // Can’t use format! because of it doesn’t work with OsString et al.
        let mut lower = OsString::from("-olowerdir=");
        lower.push(
            self.lower
                .iter()
                .map(|p| p.as_os_str())
                .collect::<Vec<_>>()
                .join(OsStr::new(":")),
        );

        let mut cmd = std::process::Command::new("mount");
        cmd.arg("-t").arg("overlay").arg("overlay");

        cmd.arg(lower);

        if let Some(upper_dir) = &self.upper_dir() {
            let mut upper = OsString::from("-oupperdir=");
            upper.push(upper_dir);

            let mut work = OsString::from("-oworkdir=");
            work.push(self.work_dir());

            cmd.arg(upper).arg(work);
        }

        cmd.arg(self.target_dir());

        let output = cmd.output().map_err(OverlayFsError::Mount)?;
        if !output.status.success() {
            return Err(OverlayFsError::Mount(std::io::Error::new(
                std::io::ErrorKind::Other,
                String::from_utf8_lossy(&output.stderr),
            )));
        }
        Ok(self.into())
    }
}

#[derive(Debug, From, Deref)]
pub struct OverlayFsGuard(OverlayFs);

impl Drop for OverlayFsGuard {
    fn drop(&mut self) {
        fn inner(fs: &OverlayFs) -> anyhow::Result<()> {
            use anyhow::Context;

            let mut cmd = std::process::Command::new("umount");
            cmd.arg(&fs.target_dir());
            let output = cmd.output().context("Failed to run `unmount`")?;
            if !output.status.success() {
                error!(
                    "Failed to unmount overlayfs: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                anyhow::bail!("Failed to unmount overlayfs");
            }

            if fs.cleanup_upper_dir {
                if let Some(upper_dir) = &fs.upper_dir() {
                    std::fs::remove_dir_all(upper_dir).context("Failed to clean up upper dir")?;
                }
            }

            if fs.cleanup_work_dir {
                std::fs::remove_dir_all(&fs.work_dir()).context("Failed to clean up work dir")?;
            }
            Ok(())
        }
        match inner(&*self) {
            Ok(_) => (),
            Err(e) => error!(
                "Failed to cleanup overlayfs {}: {e}",
                self.target_dir().display()
            ),
        }
    }
}

macro_rules! temptree {
    {$($path:literal = $content:literal;)*} => {
        {
            (|| -> ::anyhow::Result<crate::tempdir::TempDir> {
                use ::anyhow::Context;

                let tmpdir = crate::tempdir::TempDir::random();
                std::fs::create_dir_all(&*tmpdir).with_context(|| format!("Creating tempdir {}", tmpdir.display()))?;
                $({
                    let path = tmpdir.join($path);
                    let dir = path.parent().with_context(|| format!("Getting parent of {}", path.display()))?;
                    std::fs::create_dir_all(dir).with_context(|| format!("Creating directory {}", dir.display()))?;
                    std::fs::write(&path, $content).with_context(|| format!("Writing to {}", path.display()))?;
                })*
                Ok(tmpdir)
            })()
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;

    #[test]
    #[cfg_attr(not(target_os = "linux"), ignore = "overlayfs is a Linux feature")]
    fn test_readonly() -> Result<()> {
        let tmpdir = temptree! {
            "lower_1/a" = "a";
            "lower_2/b" = "b";
            "target/.empty" = "";
        }?;

        let target = tmpdir.join("target");
        let overlayfs = OverlayFs::builder()
            .lower(vec![tmpdir.join("lower_1"), tmpdir.join("lower_2")])
            .target(target.clone())
            .finish();
        {
            let _guard = overlayfs.apply()?;
            assert!(std::fs::metadata(&target.join("a"))?.is_file());
            assert!(std::fs::metadata(&target.join("b"))?.is_file());
            assert!(std::fs::write(target.join("c"), "a_new").is_err());
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
        let tmpdir = temptree! {
            "lower_1/f/a" = "1";
            "lower_1/f/b" = "1";
            "lower_1/c" = "1";
            "lower_2/f/a" = "2";
            "lower_2/f/d" = "2";
            "lower_2/c" = "2";
            "lower_2/e" = "2";
            "target/.empty" = "";
        }?;

        let target = tmpdir.join("target");
        let overlayfs = OverlayFs::builder()
            .lower(vec![tmpdir.join("lower_2"), tmpdir.join("lower_1")])
            .target(target.clone())
            .finish();

        let _guard = overlayfs.apply()?;
        assert_eq!(std::fs::read_to_string(target.join("f/a"))?, "2");
        assert_eq!(std::fs::read_to_string(target.join("f/b"))?, "1");
        assert_eq!(std::fs::read_to_string(target.join("f/d"))?, "2");
        assert_eq!(std::fs::read_to_string(target.join("c"))?, "2");
        assert_eq!(std::fs::read_to_string(target.join("e"))?, "2");
        Ok(())
    }
}
