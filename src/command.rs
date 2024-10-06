use std::{
    ffi::{CStr, CString, OsStr},
    path::PathBuf,
    process::{Command, Output},
};

use anyhow::Result;
use derive_more::derive::Deref;
use tracing::{error, instrument, trace};

pub fn resolve_command(command: impl AsRef<OsStr>) -> PathBuf {
    let command = command.as_ref();
    let Some(path) = std::env::var_os("PATH").and_then(|p| p.into_string().ok()) else {
        return command.into();
    };

    for path in path.split(':') {
        let maybe_new_command = PathBuf::from(path).join(command);
        if maybe_new_command.exists() {
            return maybe_new_command;
        }
    }
    command.into()
}

#[instrument(level = "trace", fields(
    current_dir = %command.get_current_dir().map(|v| v.to_path_buf()).or_else(|| std::env::current_dir().ok()).unwrap_or_else(|| "<unknown>".into()).display()
), ret)]
pub fn run_command(command: Command) -> Result<Output> {
    // This is a dirty hack.
    // For some reason, std::process::Command is not actually respecting $PATH
    // so I currently have to re-implement it.
    let resolved_command = resolve_command(command.get_program());
    trace!("Resolved command: {resolved_command:?}");

    let mut new_command = Command::new(resolved_command);
    new_command.args(command.get_args());
    new_command.envs(command.get_envs().filter_map(|(k, v)| Some((k, v?))));
    new_command.stdin(std::process::Stdio::piped());
    new_command.stdout(std::process::Stdio::piped());
    new_command.stderr(std::process::Stdio::piped());
    let output = new_command.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr)
            .unwrap_or_else(|_| "<Invalid UTF-8 on stderr>".to_string());
        error!("Command {command:?} failed: {stderr}");
        anyhow::bail!("Command {command:?} failed");
    }
    Ok(output)
}

pub trait ChildProcess {
    fn wait(&mut self) -> Result<Option<i32>>;
    fn kill(&mut self) -> Result<()>;
    fn pid(&self) -> u32;
}

#[derive(Debug, Deref)]
pub struct NixUnistdChild(nix::unistd::Pid);

impl ChildProcess for NixUnistdChild {
    fn wait(&mut self) -> Result<Option<i32>> {
        match nix::sys::wait::waitpid(self.0, None)? {
            nix::sys::wait::WaitStatus::Exited(_, status) => Ok(Some(status)),
            _ => Ok(None),
        }
    }

    fn kill(&mut self) -> Result<()> {
        _ = nix::sys::signal::kill(self.0, nix::sys::signal::Signal::SIGTERM);
        Ok(())
    }

    fn pid(&self) -> u32 {
        self.0.as_raw().try_into().unwrap()
    }
}

impl From<nix::unistd::Pid> for NixUnistdChild {
    fn from(pid: nix::unistd::Pid) -> Self {
        Self(pid)
    }
}

impl ChildProcess for std::process::Child {
    fn wait(&mut self) -> Result<Option<i32>> {
        Ok(self.wait()?.code())
    }

    fn kill(&mut self) -> Result<()> {
        self.kill()?;
        Ok(())
    }

    fn pid(&self) -> u32 {
        self.id() as u32
    }
}
