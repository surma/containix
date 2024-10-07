use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{bail, Result};
use tracing::{instrument, Level};

use crate::nix_helpers::NixFlake;

static HOST_TOOLS: OnceLock<PathBuf> = OnceLock::new();

#[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
pub fn setup_host_tools(host_tools: impl AsRef<str>, refresh: bool) -> Result<()> {
    let host_tools = host_tools.as_ref();
    let path = if host_tools.starts_with("/nix/store") {
        PathBuf::from(host_tools)
    } else {
        let flake: NixFlake = host_tools.parse()?;
        let flake_build = flake.build(|args| {
            args.refresh(refresh);
        })?;
        let Some(item) = flake_build.get_bin() else {
            bail!("Host tools flake did not build any packages");
        };
        item.path()
    };
    HOST_TOOLS
        .set(path)
        .expect("Global host tools path must be unset at this point");
    Ok(())
}

pub fn get_host_tools() -> &'static Path {
    HOST_TOOLS.get().expect("Host tools must be set").as_path()
}
