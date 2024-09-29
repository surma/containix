use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct X(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct NixStoreItem(String);

impl<'de> Deserialize<'de> for NixStoreItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        NixStoreItem::try_from(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl From<NixStoreItem> for PathBuf {
    fn from(val: NixStoreItem) -> Self {
        val.path()
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
}
impl Display for NixStoreItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path().display())
    }
}

impl TryFrom<&str> for NixStoreItem {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self> {
        if !value.starts_with("/nix/store/") && !value.contains('/') {
            return Ok(NixStoreItem(value.to_string()));
        }
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerConfig {
    pub flake: NixStoreItem,
    pub args: Vec<String>,
    pub interface: Option<()>,
}

const NIX_STORE_ITEM: &str = r#"
 {
  "flake": "1xh6vqbg6zgr0ks7nq9jrb94dlyvi9dl-simple-container",
  "args": [],
  "interface": null
}
"#;
fn main() -> Result<()> {
    let config = ContainerConfig {
        flake: NixStoreItem::try_from(
            "/nix/store/1xh6vqbg6zgr0ks7nq9jrb94dlyvi9dl-simple-container",
        )?,
        args: vec![],
        interface: None,
    };
    let s = serde_json::to_string(&config)?;
    println!("{s:?}");
    let y: ContainerConfig = serde_json::from_reader(std::io::Cursor::new(s))?;
    println!("{y:?}");
    Ok(())
}
