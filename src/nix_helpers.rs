use anyhow::{Context, Result};
use derive_more::derive::From;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet, ffi::OsStr, fmt::Display, path::PathBuf, process::Command, str::FromStr,
};

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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, enum_as_inner::EnumAsInner)]
#[serde(tag = "type")]
pub enum NixComponent {
    Store(NixStoreItem),
    Nixpkgs(String),
}

impl NixComponent {
    pub fn realise(self, nixpkgs: &Nixpkgs) -> Result<Self> {
        match self {
            NixComponent::Nixpkgs(component) => {
                Ok(NixComponent::from_path(nixpkgs.realise(component)?)?)
            }
            path => Ok(path),
        }
    }

    pub fn store_path(&self) -> Result<PathBuf> {
        match self {
            NixComponent::Store(component) => Ok(component.as_path()),
            NixComponent::Nixpkgs(component) => {
                anyhow::bail!("Can’t provide path for unbuilt Nixpkgs component {component}")
            }
        }
    }

    pub fn closure(&self) -> Result<HashSet<NixComponent>> {
        tracing::trace!("Getting closure for {self:?}");
        let output = Command::new("nix-store")
            .args(["--query", "--requisites"])
            .arg(self.store_path()?)
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
            .map(NixComponent::from_path)
            .collect::<Result<_>>()
            .context("Parsing nix-store output")?;

        Ok(closure)
    }

    pub fn from_path(path: impl AsRef<OsStr>) -> Result<Self> {
        use std::path::Component;
        let path = PathBuf::from(path.as_ref());
        let parts: Vec<_> = path.components().collect();
        let component = match parts.as_slice() {
            &[Component::RootDir, Component::Normal(nix), Component::Normal(store), Component::Normal(component), ..]
                if nix == "nix" && store == "store" =>
            {
                component
            }
            _ => anyhow::bail!("Path {} is not in the nix store", path.display()),
        };
        Ok(NixComponent::Store(
            component
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Nix component contains non-utf8 characters"))?
                .to_string()
                .into(),
        ))
    }
}

impl FromStr for NixComponent {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.starts_with("/") {
            NixComponent::from_path(s)
        } else {
            Ok(NixComponent::Nixpkgs(s.to_string()))
        }
    }
}

#[derive(Debug, Clone)]
pub enum Nixpkgs {
    LocalChannel(Option<String>),
    Tarball { url: url::Url, hash: Option<String> },
}

impl Display for Nixpkgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Nixpkgs::LocalChannel(_) => write!(f, "{}", self.pkg_expression()),
            Nixpkgs::Tarball { url, .. } => write!(f, "<{}>", url),
        }
    }
}

impl FromStr for Nixpkgs {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.starts_with("<") && s.ends_with(">") {
            Ok(Nixpkgs::LocalChannel(Some(s[1..s.len() - 1].to_string())))
        } else if s.starts_with("http://") || s.starts_with("https://") {
            let url = url::Url::parse(s)?;
            let hash = url
                .query_pairs()
                .find(|(key, _)| key == "sha256")
                .map(|(_, value)| value.to_string());

            Ok(Nixpkgs::Tarball { url, hash })
        } else {
            anyhow::bail!("Invalid Nixpkgs specification: {s}")
        }
    }
}

impl Nixpkgs {
    pub fn import_expression(&self) -> String {
        format!(r#"import ({}) {{}}"#, self.pkg_expression())
    }

    pub fn pkg_expression(&self) -> String {
        match self {
            Nixpkgs::LocalChannel(channel) => {
                format!(
                    "<{}>",
                    channel.as_ref().map(|c| c.as_str()).unwrap_or("nixpkgs")
                )
            }
            Nixpkgs::Tarball { url, hash } if hash.is_some() => {
                format!(
                    r#"
                        builtins.fetchTarball {{
                            url = "{url}";
                            sha256 = "{hash}";
                        }}
                    "#,
                    hash = hash.as_ref().expect("Guaranteed by match arm guard")
                )
            }
            Nixpkgs::Tarball { url, .. } => {
                format!(r#"builtins.fetchTarball "{url}""#,)
            }
        }
    }

    pub fn realise(&self, component_name: impl AsRef<str>) -> Result<PathBuf> {
        let component_name = component_name.as_ref();
        tracing::trace!("Realising `{component_name}` from {self}");

        let output = Command::new("nix-build")
            .arg("-E")
            .arg(self.import_expression())
            .arg("-A")
            .arg(component_name)
            .arg("-Q")
            .arg("--no-out-link")
            .output()
            .context("Running nix-build")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to realise Nixpkgs component: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
        Ok(path)
    }
}

pub fn combine_closures(
    exposed_components: impl IntoIterator<Item = NixComponent>,
) -> Result<HashSet<NixComponent>> {
    let mut closure = HashSet::new();
    for component in exposed_components {
        closure.extend(component.closure()?);
        closure.insert(component.clone());
    }
    Ok(closure)
}
