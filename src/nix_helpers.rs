use anyhow::{Context, Result};
use derive_more::derive::{Deref, From, Into};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Components, Path, PathBuf},
    process::Command,
    str::FromStr,
};

#[derive(Debug, Clone, Hash, Eq, PartialEq, Into, Deref)]
pub struct NixComponent(String);

impl NixComponent {
    pub fn store_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        path.push("/");
        path.push("nix");
        path.push("store");
        path.push(&self.0);
        path
    }

    pub fn closure(&self) -> Result<HashSet<NixComponent>> {
        tracing::trace!("Getting closure for {self:?}");
        let output = Command::new("nix-store")
            .args(&["--query", "--requisites"])
            .arg(self.store_path())
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
                if nix == OsString::from("nix") && store == OsString::from("store") =>
            {
                component
            }
            _ => anyhow::bail!("Path {} is not in the nix store", path.display()),
        };
        Ok(NixComponent(
            component
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Nix component contains non-utf8 characters"))?
                .to_string(),
        ))
    }
}

impl FromStr for NixComponent {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        NixComponent::from_path(s)
    }
}
