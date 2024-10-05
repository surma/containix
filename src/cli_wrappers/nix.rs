use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result};
use derive_builder::Builder;
use derive_more::derive::From;
use serde::de::DeserializeOwned;

use crate::command::run_command;

#[derive(Debug, Clone, Default, From)]
pub enum FlakeOutputSymlink {
    None,
    #[default]
    Default,
    Custom(#[from] PathBuf),
}

#[derive(Debug, Builder)]
#[builder(build_fn(name = finish, vis = ""))]
#[builder(name = "NixBuild")]
pub struct NixBuildInvocation {
    #[builder(setter(custom))]
    arg: Vec<String>,
    #[builder(default)]
    json: bool,
    #[builder(setter(into, strip_option), default)]
    lock_file: Option<PathBuf>,
    #[builder(default = "true")]
    quiet: bool,
    #[builder(default, setter(into))]
    symlink: FlakeOutputSymlink,
}

impl NixBuild {
    pub fn arg(&mut self, arg: impl ToString) -> &mut Self {
        self.arg
            .get_or_insert_with(std::vec::Vec::new)
            .push(arg.to_string());
        self
    }

    pub fn run<I: DeserializeOwned>(self) -> Result<I> {
        let invocation = self.finish()?;

        let mut cmd = Command::new("nix");
        cmd.args(&invocation.arg);

        if invocation.json {
            cmd.arg("--json");
        }

        if let Some(lock_file) = &invocation.lock_file {
            cmd.arg("--reference-lock-file")
                .arg(lock_file)
                .arg("--output-lock-file")
                .arg(lock_file);
        } else {
            cmd.arg("--no-write-lock-file");
        }

        if invocation.quiet {
            cmd.arg("--quiet");
        }

        match invocation.symlink {
            FlakeOutputSymlink::None => {
                cmd.arg("--no-link");
            }
            FlakeOutputSymlink::Custom(symlink) => {
                cmd.arg("--out-link").arg(symlink);
            }
            FlakeOutputSymlink::Default => {}
        }

        let output = run_command(cmd).context("Running nix command")?;
        let output = serde_json::from_str(&String::from_utf8(output.stdout)?)
            .context("Parsing nix output")?;
        Ok(output)
    }
}

#[derive(Debug, Builder)]
#[builder(build_fn(name = finish, vis = ""))]
#[builder(name = "NixEval")]
pub struct NixEvalInvocation {
    #[builder(default)]
    impure: bool,
    #[builder(default)]
    json: bool,
    #[builder(setter(into))]
    expression: String,
}

impl NixEval {
    pub fn run<I: DeserializeOwned>(self) -> Result<I> {
        let invocation = self.finish()?;

        let mut cmd = Command::new("nix");
        cmd.arg("eval");

        if invocation.json {
            cmd.arg("--json");
        }

        if invocation.impure {
            cmd.arg("--impure");
        }

        cmd.arg("--expr").arg(&invocation.expression);

        let output = run_command(cmd).context("Running nix command")?;
        let output = serde_json::from_str(&String::from_utf8(output.stdout)?)
            .context("Parsing nix output")?;
        Ok(output)
    }
}
