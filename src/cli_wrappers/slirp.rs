use std::{
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use anyhow::Result;
use derive_builder::Builder;
use serde::Serialize;

#[derive(Debug, Builder)]
#[builder(build_fn(name = finish, vis = ""))]
#[builder(name = "Slirp")]
pub struct SlirpInvocation {
    #[builder(setter(into))]
    binary: PathBuf,
    #[builder(setter(into))]
    pid: u32,
    #[builder(setter(into))]
    socket: PathBuf,
    #[builder(setter(custom, name = "port"))]
    ports: Vec<(u16, u16)>,
    #[builder(default = r#""tap0".into()"#)]
    device_name: String,
}

impl Slirp {
    pub fn port(&mut self, host_port: u16, guest_port: u16) -> &mut Self {
        self.ports
            .get_or_insert_with(Vec::new)
            .push((host_port, guest_port));
        self
    }

    pub fn activate(&mut self) -> Result<Child> {
        let invocation = self.finish()?;
        let mut c = Command::new(invocation.binary);
        c.arg("-c")
            .arg(invocation.pid.to_string())
            .arg(invocation.device_name)
            .arg("--api-socket")
            .arg(&invocation.socket)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let c = c.spawn()?;
        for (host_port, guest_port) in invocation.ports {
            expose_port(&invocation.socket, host_port, guest_port)?;
        }
        Ok(c)
    }
}

#[derive(Debug, Serialize)]
struct SlirpCommand<T: Serialize> {
    execute: String,
    arguments: T,
}
#[derive(Debug, Serialize)]
struct SlirpExposePortCommand {
    proto: String,
    host_addr: String,
    host_port: u16,
    guest_addr: String,
    guest_port: u16,
}
pub fn expose_port(socket: impl AsRef<Path>, host_port: u16, guest_port: u16) -> Result<()> {
    let mut stream = UnixStream::connect(socket.as_ref())?;
    let command = SlirpCommand {
        execute: "add_hostfwd".to_string(),
        arguments: SlirpExposePortCommand {
            proto: "tcp".to_string(),
            host_addr: "0.0.0.0".to_string(),
            guest_addr: "10.0.2.100".to_string(),
            host_port,
            guest_port,
        },
    };
    serde_json::to_writer(&mut stream, &command)?;
    Ok(())
}
