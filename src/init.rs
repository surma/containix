use std::{os::unix::process::CommandExt, process::Command, time::Duration};

use anyhow::{Context, Result};

use crate::{command_wrappers::Interface, ContainerConfig};

pub fn poll<T>(duration: Duration, wait: Duration, f: impl Fn() -> Result<Option<T>>) -> Result<T> {
    let start = std::time::Instant::now();
    loop {
        if let Some(result) = f()? {
            return Ok(result);
        }
        if start.elapsed() > duration {
            anyhow::bail!("Timed out");
        }
        std::thread::sleep(wait);
    }
}

pub fn initialize_container() -> Result<()> {
    tracing::info!("Starting containix in container");
    tracing::trace!("env = {:?}", std::env::vars());
    let config_path =
        std::fs::File::open("/containix.config.json").context("Opening container config")?;
    let config: ContainerConfig =
        serde_json::from_reader(config_path).context("Parsing container config")?;

    if let Some(network_config) = &config.interface {
        tracing::info!("Waiting for network interface {}", network_config.name);
        let interface = poll(Duration::from_secs(10), Duration::from_millis(100), || {
            Interface::by_name(&network_config.name)
        })?;
        interface.set_address(&network_config.address, &network_config.netmask)?;
        interface.up()?;
    }

    let err = Command::new(config.flake.path.join("bin").join(config.flake.name))
        .args(config.args)
        .current_dir("/")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .exec();

    Err(err.into())
}
