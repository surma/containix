use std::fmt;
use std::str::FromStr;

use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
}

impl fmt::Display for PortMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host_port, self.container_port)
    }
}

impl FromStr for PortMapping {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        if !s.contains(":") {
            let port: u16 = s.parse()?;
            return Ok(PortMapping {
                host_port: port,
                container_port: port,
            });
        }
        let Some((host_port, container_port)) = s.split_once(':') else {
            bail!("Invalid port mapping: {s}");
        };
        Ok(PortMapping {
            host_port: host_port.parse()?,
            container_port: container_port.parse()?,
        })
    }
}
