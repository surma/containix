use std::process::Command;

use anyhow::Result;
use nix::unistd::{getgid, getuid};

fn main() -> Result<()> {
    let uid = getuid();
    let gid = getgid();
    nix::sched::unshare(
        nix::sched::CloneFlags::CLONE_NEWUSER.union(nix::sched::CloneFlags::CLONE_NEWNS),
    )?;
    std::fs::write("/proc/self/setgroups", "deny")?;
    std::fs::write("/proc/self/uid_map", dbg!(format!("0 {} 1", uid)))?;
    std::fs::write("/proc/self/gid_map", dbg!(format!("0 {} 1", gid)))?;
    let output = Command::new("id").output()?;
    let uid = String::from_utf8(output.stdout)?;
    println!("uid: {}", uid);
    Command::new("bash").spawn()?.wait()?;
    Ok(())
}
