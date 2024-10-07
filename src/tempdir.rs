use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

use anyhow::Result;
use tracing::{error, instrument};

#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Result<Self> {
        let name = uuid::Uuid::new_v4().to_string();
        Self::with_name(Option::<&str>::None, name)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn with_name(prefix: Option<impl AsRef<str>>, suffix: impl AsRef<str>) -> Result<Self> {
        let mut name = String::new();
        if let Some(prefix) = prefix {
            name.push_str(prefix.as_ref());
            name.push('-');
        }
        name.push_str(suffix.as_ref());
        let path = std::env::temp_dir().join(name);
        Ok(Self(path))
    }

    pub fn with_prefix(prefix: impl AsRef<str>) -> Result<Self> {
        let name = uuid::Uuid::new_v4().to_string();
        Self::with_name(Some(prefix), name)
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            error!("Failed to remove tempdir {}: {e}", self.0.display());
        }
    }
}
