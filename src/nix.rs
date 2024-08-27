use anyhow::Result;
use std::{path::PathBuf, process::Command};

pub fn get_nix_closure(binary_path: &PathBuf) -> Result<Vec<PathBuf>> {
    let output = Command::new("nix-store")
        .args(&["--query", "--requisites"])
        .arg(binary_path.as_os_str())
        .output()?;

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

pub fn find_nix_store_path(command: &str) -> Option<PathBuf> {
    let commands = which::which_all_global(command).ok()?;
    for command in commands {
        let real_path = std::fs::canonicalize(&command).ok()?;
        if real_path.starts_with("/nix/store") {
            return Some(real_path);
        }
    }
    None
}
