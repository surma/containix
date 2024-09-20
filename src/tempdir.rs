use std::{mem::ManuallyDrop, path::PathBuf};

use derive_more::derive::{Deref, DerefMut, Into};

#[derive(Debug, Deref, DerefMut)]
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tmpdir-{}", name));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn random() -> Self {
        Self::new(&uuid::Uuid::new_v4().to_string())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        _ = std::fs::remove_dir_all(&self.0);
    }
}
