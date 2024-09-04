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

use crate::command::run_command;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, From)]
pub struct NixStoreItem(String);

impl NixStoreItem {
    pub fn as_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        path.push("/");
        path.push("nix");
        path.push("store");
        path.push(&self.0);
        path
    }

    pub fn closure(&self) -> Result<HashSet<NixStoreItem>> {
        tracing::trace!("Getting closure for {self:?}");
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
            .map(|p| PathBuf::from(p).try_into())
            .collect::<Result<_>>()
            .context("Parsing nix-store output")?;

        Ok(closure)
    }
}

impl TryFrom<PathBuf> for NixStoreItem {
    type Error = anyhow::Error;

    fn try_from(value: PathBuf) -> Result<Self> {
        value.as_path().try_into()
    }
}

impl TryFrom<&Path> for NixStoreItem {
    type Error = anyhow::Error;

    fn try_from(value: &Path) -> Result<Self> {
        let components: Vec<_> = value.components().collect();
        let component = match components.as_slice() {
            &[Component::RootDir, Component::Normal(p1), Component::Normal(p2), Component::Normal(component)]
                if p1 == "nix" && p2 == "store" =>
            {
                component
            }
            _ => anyhow::bail!("Path {} is not in the nix store", value.display()),
        };
        let component = component
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Nix component contains non-utf8 characters"))?;
        Ok(NixStoreItem(component.to_string()))
    }
}

#[derive(Debug, Clone, EnumAsInner)]
pub enum NixBuild {
    LocalFile(PathBuf),
    // FlakePath(PathBuf),
    FlakeExpression {
        installable: String,
        includes: Option<HashMap<String, String>>,
    },
}

impl Display for NixBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NixBuild::LocalFile(path) => write!(f, "{}", path.to_string_lossy()),
            NixBuild::FlakeExpression { installable, .. } => write!(f, "{}", installable),
        }
    }
}

impl FromStr for NixBuild {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.ends_with(".nix") && !s.ends_with("flake.nix") {
            Ok(NixBuild::LocalFile(PathBuf::from(s)))
        } else {
            Ok(NixBuild::FlakeExpression {
                installable: s.to_string(),
                includes: None,
            })
        }
    }
}

impl NixBuild {
    pub fn build(&self) -> Result<NixStoreItem> {
        match self {
            NixBuild::LocalFile(path) => build_nix_file(path),
            // NixBuild::FlakePath(path) => NixComponent::from_path(path),
            NixBuild::FlakeExpression {
                includes,
                installable,
            } => build_nix_flake(installable, includes.as_ref()),
        }
    }

    pub fn nixpkg_component(component_name: impl AsRef<str>, nixpkgs: impl AsRef<str>) -> Self {
        NixBuild::FlakeExpression {
            installable: format!("nixpkgs#{}", component_name.as_ref()),
            includes: Some([("nixpkgs".to_string(), nixpkgs.as_ref().to_string())].into()),
        }
    }
}

pub fn build_nix_file(nix_file_path: impl AsRef<Path>) -> Result<NixStoreItem> {
    let nix_file_path = nix_file_path.as_ref();
    tracing::trace!("Building nix file {}", nix_file_path.display());

    let mut command = Command::new("nix-build");
    command.arg(nix_file_path).arg("-Q").arg("--no-out-link");

    let output = run_command(command).context("Running nix-build")?;
    let path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    Ok(path.try_into()?)
}

// [{"drvPath":"/nix/store/dyvxc26k9h602ad6d272b9a6n70vpn0k-ps-adv_cmds-119.drv","outputs":{"out":"/nix/store/a4pw6inaxbfry54v3dl5m152442fg4dr-ps-adv_cmds-119"}}]

#[derive(Debug, Deserialize)]
struct NixFlakeBuildOutput {
    #[serde(rename = "drvPath")]
    drv_path: PathBuf,
    outputs: HashMap<String, PathBuf>,
}

pub fn build_nix_flake(
    flake_url: impl AsRef<str>,
    includes: Option<&HashMap<String, String>>,
) -> Result<NixStoreItem> {
    let flake_url = flake_url.as_ref();
    tracing::trace!("Building nix flake {}", flake_url);

    let mut command = Command::new("nix");
    command
        .arg("build")
        .arg(flake_url)
        .arg("--json")
        .arg("--quiet")
        .arg("--no-link");
    if let Some(includes) = includes {
        for (name, value) in includes {
            command.arg("-I").arg(format!("{name}={value}"));
        }
    }

    let output = run_command(command).context("Running nix-build")?;
    let output: Vec<NixFlakeBuildOutput> =
        serde_json::from_str(&String::from_utf8(output.stdout)?)?;
    let output = output.get(0).context("No output items from nix build")?;

    Ok(output
        .outputs
        .get("bin")
        .or_else(|| output.outputs.get("out"))
        .context("No suitable output in nix build")?
        .as_path()
        .try_into()?)
}

pub fn combine_closures<'a>(
    exposed_components: impl IntoIterator<Item = &'a NixStoreItem>,
) -> Result<HashSet<NixStoreItem>> {
    let mut closure = HashSet::new();
    for component in exposed_components {
        closure.extend(component.closure()?);
        closure.insert(component.clone());
    }
    Ok(closure)
}
