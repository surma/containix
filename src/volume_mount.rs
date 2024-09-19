use std::{path::PathBuf, str::FromStr};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct VolumeMount {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
}

impl FromStr for VolumeMount {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let Some((host_path, container_path)) = s.split_once(':') else {
            anyhow::bail!(
                "Volume mount must be of the form <HOST PATH>:<CONTAINER PATH>, got: {s}"
            );
        };
        Ok(VolumeMount {
            host_path: host_path.into(),
            container_path: container_path.into(),
        })
    }
}
