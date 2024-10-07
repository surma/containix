use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct VolumeMount {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
    pub read_only: bool,
}

impl VolumeMount {
    pub fn read_only(host_path: impl AsRef<Path>, container_path: impl AsRef<Path>) -> Self {
        Self {
            host_path: host_path.as_ref().to_path_buf(),
            container_path: container_path.as_ref().to_path_buf(),
            read_only: true,
        }
    }
}

impl FromStr for VolumeMount {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let Some((host_path, container_path)) = s.split_once(':') else {
            anyhow::bail!(
                "Volume mount must be of the form <HOST PATH>:<CONTAINER PATH>[:<OPTIONS>], got: {s}"
            );
        };
        let (container_path, options) = container_path
            .split_once(':')
            .unwrap_or((container_path, ""));
        let options: Vec<_> = options.split(',').collect();
        let read_only = options.iter().any(|option| *option == "ro");
        Ok(VolumeMount {
            host_path: host_path.into(),
            container_path: container_path.into(),
            read_only,
        })
    }
}
