use anyhow::{anyhow, Context, Result};
use derive_more::derive::{Deref, DerefMut, From};
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::PathBuf,
    process::Command,
    str::FromStr,
};
use tracing::{debug, instrument, trace};

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

    #[instrument(level = "trace", skip_all, fields(path = %self.as_path().display()))]
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
            .map(PathBuf::from)
            .collect();

        Ok(closure)
    }
}

#[derive(Debug, Clone)]
pub struct NixFlake {
    flake: String,
    output: Option<String>,
}

impl Display for NixFlake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.flake)?;
        if let Some(output) = &self.output {
            write!(f, "#{output}")?;
        }
        Ok(())
    }
}

impl FromStr for NixFlake {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if let Some((flake, output)) = s.split_once('#') {
            Ok(Self {
                flake: flake.to_string(),
                output: Some(output.to_string()),
            })
        } else {
            Ok(Self {
                flake: s.to_string(),
                output: None,
            })
        }
    }
}

impl NixFlake {
    #[instrument(level = "trace", skip_all)]
    pub fn build(&self) -> Result<NixStoreItem> {
        build_nix_flake_container(&self.flake, self.output.as_ref())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn info(&self) -> Result<NixFlakeShowOutput> {
        let output: NixFlakeShowOutput = crate::cli_wrappers::nix::Nix::builder()
            .arg("flake")
            .arg("show")
            .arg(self)
            .json()
            .run()?;

        Ok(output)
    }

    pub fn output_from_flake(output_name: impl AsRef<str>, flake: impl AsRef<str>) -> Self {
        Self {
            flake: flake.as_ref().to_string(),
            output: Some(output_name.as_ref().to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NixFlakeShowOutput {
    pub packages: Option<NixFlakePackages>,
    pub legacy_packages: Option<NixFlakePackages>,
    // Other items emitted
}

impl NixFlakeShowOutput {
    pub fn find_package(
        &self,
        system: &NixSystem,
        package_name: &str,
    ) -> Option<(String, String, &HashMap<String, serde_json::Value>)> {
        if let Some(packages) = &self.packages {
            packages
                .get(system)
                .and_then(|packages| packages.get(package_name))
                .map(|package| ("packages".to_string(), package_name.to_string(), package))
        } else if let Some(legacy_packages) = &self.legacy_packages {
            legacy_packages
                .get(system)
                .and_then(|packages| packages.get(package_name))
                .map(|package| {
                    (
                        "legacyPackages".to_string(),
                        package_name.to_string(),
                        package,
                    )
                })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Deserialize, Deref, DerefMut)]
pub struct NixFlakePackages(
    HashMap<NixSystem, HashMap<String, HashMap<String, serde_json::Value>>>,
);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct NixSystem {
    architecture: String,
    os: String,
}

impl Display for NixSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.architecture, self.os)
    }
}

impl FromStr for NixSystem {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let Some((architecture, os)) = s.split_once('-') else {
            anyhow::bail!("Invalid Nix system string: {s}");
        };

        Ok(NixSystem {
            architecture: architecture.to_string(),
            os: os.to_string(),
        })
    }
}

impl<'de> Deserialize<'de> for NixSystem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        NixSystem::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
pub struct NixFlakeBuildOutput {
    #[serde(rename = "drvPath")]
    drv_path: PathBuf,
    outputs: HashMap<String, PathBuf>,
}

#[instrument(level = "trace", ret)]
pub fn get_nix_system() -> Result<NixSystem> {
    let mut command = Command::new("nix");
    command
        .arg("eval")
        .arg("--impure")
        .arg("--expr")
        .arg("builtins.currentSystem");

    let output = run_command(command).context("Running nix eval")?;
    let system = serde_json::from_str(&String::from_utf8(output.stdout)?)
        .context("Failed to parse nix system")?;
    Ok(system)
}

pub fn get_flake_info(flake_expression: impl AsRef<str>) -> Result<NixFlakeShowOutput> {
    let mut command = Command::new("nix");
    command
        .arg("flake")
        .arg("show")
        .arg(flake_expression.as_ref())
        .arg("--json")
        // FIXME:
        // The code that builds packages checks that it is actually present on the flake. This is probably a bad idea for nixpkgs, but for now I force all packages to be listed.
        .arg("--legacy")
        .arg("--reference-lock-file")
        .arg("containix.lock")
        .arg("--output-lock-file")
        .arg("containix.lock")
        .arg("--quiet");

    let output = run_command(command).context("Running nix flake show")?;
    let output: NixFlakeShowOutput = serde_json::from_str(&String::from_utf8(output.stdout)?)
        .context("Analyzing nix flake show output")?;
    Ok(output)
}

#[instrument(level = "trace", skip_all, fields(flake_expression = %flake_expression.as_ref(), output = %output.as_ref().map(|v| v.as_ref()).unwrap_or("")))]
pub fn build_nix_flake_container(
    flake_expression: impl AsRef<str>,
    output: Option<impl AsRef<str>>,
) -> Result<NixStoreItem> {
    let flake_expression = flake_expression.as_ref();

    let nix_system = get_nix_system()?;
    let flake = get_flake_info(flake_expression)?;

    let (package_collection, component, package) = if let Some(component) = output {
        let component = component.as_ref().to_string();
        flake
            .find_package(&nix_system, &component)
            .ok_or_else(|| anyhow!("No package named {component} found in flake"))?
    } else {
        flake
            .find_package(&nix_system, "containix")
            .or_else(|| flake.find_package(&nix_system, "default"))
            .ok_or_else(|| anyhow!("No suitable package found in flake"))?
    };
    debug!("Building package {package_collection}.{component}");

    let outputs = build_nix_flake(
        flake_expression,
        package_collection,
        &nix_system,
        &component,
    )?;

    let name = package
        .get("name")
        .and_then(|v| v.as_str())
        .context("No name on flake output")?
        .to_string();

    let output = outputs
        .outputs
        .get("bin")
        .or_else(|| outputs.outputs.get("out"))
        .context("No output items called bin or out on flake")?;

    Ok(NixStoreItem {
        name,
        path: output.clone(),
    })
}

pub fn build_nix_flake(
    flake_expression: impl AsRef<str>,
    collection: impl AsRef<str>,
    nix_system: &NixSystem,
    package_name: impl AsRef<str>,
) -> Result<NixFlakeBuildOutput> {
    let package_name = package_name.as_ref();
    let collection = collection.as_ref();
    let flake_expression = flake_expression.as_ref();

    let outputs = {
        let mut command = Command::new("nix");
        command
            .arg("build")
            .arg(&format!(
                "{flake_expression}#{collection}.{nix_system}.{package_name}"
            ))
            .arg("--json")
            .arg("--quiet")
            .arg("--reference-lock-file")
            .arg("containix.lock")
            .arg("--output-lock-file")
            .arg("containix.lock")
            .arg("--no-link");

        let output = run_command(command).context("Running nix build")?;
        let mut output: Vec<NixFlakeBuildOutput> =
            serde_json::from_str(&String::from_utf8(output.stdout)?)
                .context("Analyzing nix build output")?;
        trace!("nix build output: {output:?}");
        output.swap_remove(0)
    };

    Ok(outputs)
}
