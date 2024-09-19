use anyhow::{Context, Result};
use derive_more::derive::From;
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fmt::Display,
    path::{Component, Path, PathBuf},
    process::Command,
    str::FromStr,
};
use tracing::{instrument, trace};

use crate::command::run_command;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, From)]
pub struct NixStoreItem {
    pub name: String,
    pub path: PathBuf,
}

impl NixStoreItem {
    pub fn as_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        path.push("/");
        path.push("nix");
        path.push("store");
        path.push(&self.path);
        path
    }

    #[instrument(level = "trace", skip_all, fields(path = %self.as_path().display()), ret)]
    pub fn closure(&self) -> Result<HashSet<PathBuf>> {
        let output = Command::new("nix-store")
            .args(["--query", "--requisites"])
            .arg(self.as_path())
            .output()
            .context("Running nix-store query for closure")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to get nix store closure: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let closure = String::from_utf8(output.stdout)?
            .lines()
            .map(|p| PathBuf::from(p))
            .collect();

        Ok(closure)
    }
}

#[derive(Debug, Clone, EnumAsInner)]
pub enum NixDerivation {
    FlakeExpression(String),
}

impl Display for NixDerivation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NixDerivation::FlakeExpression(expr) => write!(f, "{}", expr),
        }
    }
}

impl FromStr for NixDerivation {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(NixDerivation::FlakeExpression(s.to_string()))
    }
}

impl NixDerivation {
    #[instrument(level = "trace", ret)]
    pub fn build(&self) -> Result<NixStoreItem> {
        match self {
            NixDerivation::FlakeExpression(installable) => build_nix_flake(installable),
        }
    }

    pub fn package_from_flake(component_name: impl AsRef<str>, flake: impl AsRef<str>) -> Self {
        NixDerivation::FlakeExpression(format!("{}#{}", flake.as_ref(), component_name.as_ref()))
    }
}

#[derive(Debug, Deserialize)]
struct NixFlakeBuildOutput {
    #[serde(rename = "drvPath")]
    drv_path: PathBuf,
    outputs: HashMap<String, PathBuf>,
}

#[instrument(level = "trace", skip_all, fields(flake_expression = %flake_expression.as_ref()), ret)]
pub fn build_nix_flake(flake_expression: impl AsRef<str>) -> Result<NixStoreItem> {
    let flake_expression = flake_expression.as_ref();

    let outputs = {
        let mut command = Command::new("nix");
        command
            .arg("build")
            .arg(flake_expression)
            .arg("--json")
            .arg("--quiet")
            .arg("--no-link");

        let output = run_command(command).context("Running nix build")?;
        let mut output: Vec<NixFlakeBuildOutput> =
            serde_json::from_str(&String::from_utf8(output.stdout)?)
                .context("Analyzing nix build output")?;
        trace!("nix build output: {output:?}");
        output.swap_remove(0)
    };

    let output = outputs
        .outputs
        .get("bin")
        .or_else(|| outputs.outputs.get("out"))
        .context("No output items called bin or out on flake")?;

    let name = output
        .as_os_str()
        .to_str()
        .context("Nix output path contains non-utf8 characters")?
        .split_once('-')
        .context("Nix output path does not contain a hyphen")?
        .1
        .to_string();

    Ok(NixStoreItem {
        name,
        path: output.clone(),
    })
}
