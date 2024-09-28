use std::{io::Read, os::unix::process::CommandExt, process::Command, time::Duration};

use anyhow::{Context, Result};
use tracing::{debug, info, instrument, trace};

use crate::{command_wrappers::Interface, ContainerConfig};

#[instrument(level = "trace", skip_all)]
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

#[instrument(level = "trace", skip_all)]
pub fn initialize_container() -> Result<()> {
    info!("Starting containix in container");
    trace!("env = {:?}", std::env::vars());
    let config_path =
        std::fs::File::open("/containix.config.json").context("Opening container config")?;
    let config: ContainerConfig =
        serde_json::from_reader(config_path).context("Parsing container config")?;

    if let Some(network_config) = &config.interface {
        info!("Waiting for network interface {}", network_config.name);
        let interface = poll(Duration::from_secs(10), Duration::from_millis(100), || {
            Interface::by_name(&network_config.name)
        })?;
        interface.set_address(&network_config.address, &network_config.netmask)?;
        interface.up()?;
    }

    let mut cmd = Command::new(config.flake.path().join("bin").join(config.flake.name()));
    cmd.args(config.args)
        .current_dir("/")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    info!(
        "Running container command {}",
        cmd.get_program().to_string_lossy()
    );
    let err = cmd.exec();

    Err(err.into())
}
