use std::{
    fs::File,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use derive_builder::Builder;
use serde::Serialize;
use tracing::{error, instrument, trace, Level};

use crate::{command::ChildProcess, ports::PortMapping};

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
    #[builder(default = "vec![]", setter(custom, name = "port"))]
    ports: Vec<PortMapping>,
    #[builder(default = r#""tap0".into()"#)]
    device_name: String,
}

impl Slirp {
    pub fn port(&mut self, port_mapping: PortMapping) -> &mut Self {
        self.ports.get_or_insert_with(Vec::new).push(port_mapping);
        self
    }

    #[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
    pub fn activate(&mut self) -> Result<impl ChildProcess> {
        let invocation = self.finish()?;

        let (rx, tx) = nix::unistd::pipe().context("Creating ready signal pipe for slirp")?;
        let mut c = Command::new(invocation.binary);
        c.arg("-c")
            .arg(invocation.pid.to_string())
            .arg(invocation.device_name)
            .arg("--api-socket")
            .arg(&invocation.socket)
            .arg("--ready-fd")
            .arg(tx.as_raw_fd().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let c = c.spawn().context("Spawning slirp")?;
        trace!("Slirp spawned with PID {}", c.pid());
        std::thread::spawn(move || {
            if let Err(e) =
                intialize_with_ports(File::from(rx), invocation.socket, &invocation.ports)
            {
                error!("Error initializing slirp: {e}");
            }
        });
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
fn expose_port(socket: impl AsRef<Path>, host_port: u16, guest_port: u16) -> Result<()> {
    let mut stream = UnixStream::connect(socket.as_ref()).context("Connecting to slirp socket")?;
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
    // Commands must be sent in one packet, so do NOT use `to_writer` here.
    let cmd = serde_json::to_string(&command).context("Serializing slirp command")?;
    stream
        .write_all(cmd.as_bytes())
        .context("Sending slirp command")?;
    Ok(())
}

fn wait_for_slirp_ready(mut signal: impl Read) -> Result<()> {
    let mut buf = [0; 1];
    while let Ok(0) = signal.read(&mut buf) {}
    Ok(())
}

fn intialize_with_ports<'a>(
    signal: impl Read,
    socket: impl AsRef<Path>,
    ports: impl IntoIterator<Item = &'a PortMapping>,
) -> Result<()> {
    wait_for_slirp_ready(signal).context("Waiting for slirp to initialize")?;
    for port in ports.into_iter() {
        expose_port(&socket, port.host_port, port.container_port).context("Exposing ports")?;
    }
    Ok(())
}
