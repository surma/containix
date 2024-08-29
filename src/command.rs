use std::process::{Command, Output};

use anyhow::{Context, Result};

pub fn run_command(mut command: Command) -> Result<Output> {
    let output = command.output().context("Running mount")?;
    if !output.status.success() {
        anyhow::bail!(
            "Failed to run {command:?}: {}",
            &String::from_utf8(output.stderr)
                .unwrap_or_else(|_| "<Invalid UTF-8 on stderr>".to_string())
        );
    }
    Ok(output)
}
