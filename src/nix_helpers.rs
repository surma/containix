use anyhow::{anyhow, bail, Context, Result};
use derive_more::derive::{Deref, DerefMut, From};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fmt::Display,
    path::{Component, Path, PathBuf},
    process::Command,
    str::FromStr,
};
use tracing::{debug, error, instrument, trace};

use crate::{
    cli_wrappers::nix::{FlakeOutputSymlink, NixBuild, NixEval},
    command::run_command,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct NixStoreItem(String);

impl Into<PathBuf> for NixStoreItem {
    fn into(self) -> PathBuf {
        self.path()
    }
}

impl<'de> Deserialize<'de> for NixStoreItem {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let s = String::deserialize(deserializer)?;
        Ok(NixStoreItem::try_from(s.as_str()).map_err(|err| D::Error::custom(err))?)
    }
}

impl TryFrom<&str> for NixStoreItem {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self> {
        let components: Vec<_> = value.split('/').collect();
        let &["", "nix", "store", item] = components.as_slice() else {
            bail!("{} is not a nix store item", value);
        };
        Ok(NixStoreItem(item.to_string()))
    }
}

impl TryFrom<&Path> for NixStoreItem {
    type Error = anyhow::Error;
    fn try_from(value: &Path) -> Result<Self> {
        let Some(str) = value.to_str() else {
            bail!("Path {} contains non-utf8", value.display());
        };
        str.try_into()
    }
}

impl NixStoreItem {
    pub fn path(&self) -> PathBuf {
        PathBuf::from("/nix/store").join(&self.0)
    }

    pub fn components(&self) -> (&str, &str) {
        self.0
            .split_once('-')
            .unwrap_or_else(|| panic!("Invalid nix store path"))
    }

    pub fn name(&self) -> &str {
        self.components().1
    }

    #[instrument(level = "trace", skip_all, fields(path = %self.path().display()))]
    pub fn closure(&self) -> Result<HashSet<NixStoreItem>> {
        let output = Command::new("nix-store")
            .args(["--query", "--requisites"])
            .arg(self.path())
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
            .map(NixStoreItem::try_from)
            .collect::<Result<_>>()?;

        Ok(closure)
    }
}

#[derive(Debug, Clone, Deref, DerefMut)]
pub struct ContainixFlake(NixFlake);

impl FromStr for ContainixFlake {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(ContainixFlake(s.parse()?))
    }
}

impl ContainixFlake {
    pub fn build(&self) -> Result<NixStoreItem> {
        static DEFAULT_OUTPUT_NAMES: &[&str] = &["containix", "default"];

        let c = if self.output().is_none() {
            let system = get_nix_system()?;
            let info = self.info()?;
            let Some(packages) = info.packages.as_ref().and_then(|p| p.get(&system)) else {
                bail!("Container flake has no packages for {}", system);
            };
            let Some(output) = DEFAULT_OUTPUT_NAMES
                .iter()
                .find(|name| packages.contains_key(**name))
            else {
                error!(
                    "Container flake outputs ({}) do not contain one of the expected outputs ({})",
                    packages
                        .keys()
                        .map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Vec::from(DEFAULT_OUTPUT_NAMES).join(", ")
                );
                bail!("Container flake does not provide expected output");
            };
            ContainixFlake(self.with_output(format!("packages.{system}.{output}")))
        } else {
            self.clone()
        };

        let build = c.0.build(|nix_cmd: &mut NixBuild| {
            nix_cmd
                .lock_file("containix.lock")
                .symlink(FlakeOutputSymlink::None);
        })?;

        let Some(path) = build.get_bin() else {
            bail!("Container flake did not provide a bin or out");
        };

        Ok(path.clone())
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
    pub fn build<F>(&self, f: F) -> Result<NixBuildResult>
    where
        F: FnOnce(&mut NixBuild),
    {
        let mut nix_cmd = NixBuild::default();
        nix_cmd.arg("build").arg(self.to_string()).json(true);
        f(&mut nix_cmd);
        let mut output: Vec<NixFlakeBuildOutput> = nix_cmd.run()?;

        if output.len() > 1 {
            debug!("{output:?}");
            bail!("Flake unexpectedly built more than one output derivation");
        }

        Ok(NixBuildResult(output.swap_remove(0).outputs))
    }

    pub fn output(&self) -> Option<&str> {
        self.output.as_ref().map(|v| v.as_str())
    }

    pub fn with_output(&self, package_name: impl AsRef<str>) -> Self {
        NixFlake {
            flake: self.flake.clone(),
            output: Some(package_name.as_ref().to_string()),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn info(&self) -> Result<NixFlakeShowOutput> {
        let mut nix_cmd = NixBuild::default();
        nix_cmd.arg("flake").arg("show").arg(self).json(true);
        let output: NixFlakeShowOutput = nix_cmd.run()?;
        Ok(output)
    }

    pub fn output_from_flake(output_name: impl AsRef<str>, flake: impl AsRef<str>) -> Self {
        Self {
            flake: flake.as_ref().to_string(),
            output: Some(output_name.as_ref().to_string()),
        }
    }
}

#[derive(Debug, Clone, Deref)]
pub struct NixBuildResult(HashMap<String, NixStoreItem>);

impl NixBuildResult {
    pub fn get_out(&self) -> Option<&NixStoreItem> {
        self.get("out")
    }

    pub fn get_bin(&self) -> Option<&NixStoreItem> {
        self.get_or_out("bin")
    }

    /// Get a specified key or use `out` if it doesn’t exist.
    pub fn get_or_out(&self, key: impl AsRef<str>) -> Option<&NixStoreItem> {
        if let Some(out) = self.get(key.as_ref()) {
            return Some(out);
        }
        self.get_out()
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
    outputs: HashMap<String, NixStoreItem>,
}

#[instrument(level = "trace", ret)]
pub fn get_nix_system() -> Result<NixSystem> {
    let mut nix_cmd = NixEval::default();
    nix_cmd.impure(true).expression("builtins.currentSystem");

    let system: NixSystem = nix_cmd.run()?;
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
