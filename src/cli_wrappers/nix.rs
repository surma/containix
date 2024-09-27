use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Deserialize};
use typed_builder::TypedBuilder;

use crate::command::run_command;

#[derive(Debug, TypedBuilder)]
#[builder(mutators(
    pub fn arg(&mut self, v: impl ToString) {
        self.command.push(v.to_string());
    }
))]
#[builder(build_method(name = finish, vis = ""))]
pub struct Nix {
    #[builder(via_mutators(init = vec![]))]
    command: Vec<String>,
    #[builder(setter(strip_bool))]
    json: bool,
    #[builder(default, setter(strip_option))]
    lock_file: Option<PathBuf>,
    #[builder(default = true)]
    quiet: bool,
}

#[allow(dead_code, non_camel_case_types, missing_docs)]
impl<
        __json: ::typed_builder::Optional<bool>,
        __lock_file: ::typed_builder::Optional<Option<PathBuf>>,
        __quiet: ::typed_builder::Optional<bool>,
    > NixBuilder<((Vec<String>,), __json, __lock_file, __quiet)>
{
    #[allow(
        clippy::default_trait_access,
        clippy::used_underscore_binding,
        clippy::no_effect_underscore_binding
    )]
    pub fn run<I: DeserializeOwned>(self) -> Result<I> {
        let cmd_opts = self.finish();

        let mut command = Command::new("nix");
        command.args(&cmd_opts.command);
        if cmd_opts.json {
            command.arg("--json");
        }
        if let Some(lock_file) = &cmd_opts.lock_file {
            command
                .arg("--reference-lock-file")
                .arg(lock_file)
                .arg("--output-lock-file")
                .arg(lock_file);
        }
        if cmd_opts.quiet {
            command.arg("--quiet");
        }

        let output = run_command(command).context("Running nix command")?;
        let output = serde_json::from_str(&String::from_utf8(output.stdout)?)
            .context("Parsin nix output")?;
        Ok(output)
    }
}
