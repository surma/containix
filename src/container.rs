use anyhow::{Context, Result};

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Container {
    root: PathBuf,
    mounts: Vec<PathBuf>,
    keep: bool,
}

impl Container {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            mounts: Vec::new(),
            keep: false,
        }
    }

    pub fn set_keep(&mut self, keep: bool) {
        self.keep = keep;
    }

    pub fn temp_container() -> Self {
        let container_id = uuid::Uuid::new_v4().to_string();
        let temp_dir = std::env::temp_dir().join("containix").join(container_id);
        Self::new(temp_dir)
    }

    pub fn bind_mount(
        &mut self,
        src: impl AsRef<Path>,
        target: impl AsRef<Path>,
        read_only: bool,
    ) -> Result<()> {
        let src = src.as_ref();
        let target = target.as_ref();
        let target = target.strip_prefix("/").unwrap_or(target);
        let target_dir = self.root.join(&target);
        tracing::trace!("Binding mount {src:?} -> {target_dir:?}");
        std::fs::create_dir_all(&target_dir).context("Creating directory for bind mount")?;

        let status = std::process::Command::new("mount")
            .arg("-o")
            .arg(if read_only { "bind,ro" } else { "bind" })
            .arg(src)
            .arg(&target_dir)
            .status()
            .context("Running mount")?;

        if !status.success() {
            anyhow::bail!("Failed to bind mount {}", src.display());
        }
        self.mounts.push(target.to_path_buf());
        Ok(())
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        if self.keep {
            tracing::warn!(
                "Keeping container at {}, not cleaning up",
                self.root.display()
            );
            return;
        }

        for mount in &self.mounts {
            let target_dir = self.root.join(&mount);
            let status = std::process::Command::new("umount")
                .arg(&target_dir)
                .status();
            if let Err(e) = status {
                tracing::error!(
                    "Failed cleaning up bind mount {}: {e}",
                    target_dir.display()
                );
            }
        }
    }
}
