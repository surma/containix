use std::{
    ffi::{CStr, CString},
    fs::File,
    io::{Read, Write},
};

use containix::{
    command::ChildProcess,
    unshare::{UnshareEnvironmentBuilder, UnshareNamespaces},
};
use tracing::{info, Level};

#[tracing::instrument(skip_all)]
fn print_info(name: impl AsRef<str>) {
    info!(
        "{}: UID: {}, PID: {}",
        name.as_ref(),
        nix::unistd::getuid(),
        nix::unistd::getpid()
    );
}
fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();
    print_info("Pre mount NS");
    UnshareEnvironmentBuilder::default()
        .namespace(UnshareNamespaces::User)
        .namespace(UnshareNamespaces::Mount)
        .map_current_user_to_root()
        .enter()
        .unwrap();
    print_info("Post mount NS");

    let (rx, tx) = nix::unistd::pipe().unwrap();
    let mut rx = File::from(rx);
    let mut tx = Some(File::from(tx));
    let mut x = UnshareEnvironmentBuilder::default()
        .namespace(UnshareNamespaces::User)
        .namespace(UnshareNamespaces::Pid)
        .map_current_user_to_root()
        .execute(move || {
            print_info("In new PID NS");
            if let Some(mut gs) = tx.take() {
                _ = gs.write(&[0]).unwrap();
                drop(gs);
            }
            nix::unistd::execv(
                CString::new("/bin/bash").unwrap().as_c_str(),
                &[] as &[&'static CStr],
            )
            .unwrap();
            unreachable!()
        })
        .unwrap();
    let mut buf = [0; 1];
    rx.read_exact(&mut buf).unwrap();
    info!("Child pid: {}", x.pid());
    // Without this sleep, the wait() sometimes fails with `ECHILD`, which seems odd.
    std::thread::sleep(std::time::Duration::from_millis(100));
    x.wait().unwrap();
    info!("Done!");
}
