use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use derive_builder::Builder;
use derive_more::derive::{Deref, DerefMut};
use tracing::{instrument, Level};

use crate::container::ContainerHandle;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum UnshareNamespaces {
    /// Mounting and unmounting filesystems will not affect the rest of the system.
    Mount,
    /// Setting hostname or domainname will not affect the rest of the system.
    Uts,
    /// The process will have an independent namespace for POSIX message queues as well as System V message queues, semaphore sets and shared memory segments
    Ipc,
    /// The process will have independent IPv4 and IPv6 stacks, IP routing tables, firewall rules, the `/proc/net` and `/sys/class/net` directory trees, sockets, etc.
    Network,
    /// Children will have a distinct set of PID-to-process mappings from their parent.
    Pid,
    /// The process will have a virtualized view of `/proc/self/cgroup`, and new cgroup mounts will be rooted at the namespace cgroup root.
    Cgroup,
    /// The process will have a distinct set of UIDs, GIDs and capabilities.
    User,
    /// The process can have a distinct view of CLOCK_MONOTONIC and/or CLOCK_BOOTTIME which can be changed using `/proc/self/timens_offsets`.
    Time,
}

impl From<UnshareNamespaces> for nix::sched::CloneFlags {
    fn from(val: UnshareNamespaces) -> Self {
        match val {
            UnshareNamespaces::Mount => nix::sched::CloneFlags::CLONE_NEWNS,
            UnshareNamespaces::Uts => nix::sched::CloneFlags::CLONE_NEWUTS,
            UnshareNamespaces::Ipc => nix::sched::CloneFlags::CLONE_NEWIPC,
            UnshareNamespaces::Network => nix::sched::CloneFlags::CLONE_NEWNET,
            UnshareNamespaces::Pid => nix::sched::CloneFlags::CLONE_NEWPID,
            UnshareNamespaces::Cgroup => nix::sched::CloneFlags::CLONE_NEWCGROUP,
            UnshareNamespaces::User => nix::sched::CloneFlags::CLONE_NEWUSER,
            UnshareNamespaces::Time => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdRangeMap {
    outer_id_start: u32,
    inner_id_start: u32,
    count: u32,
}

impl IdRangeMap {
    pub fn serialize(&self) -> String {
        format!(
            "{} {} {}",
            self.inner_id_start, self.outer_id_start, self.count
        )
    }
}

#[derive(Debug, Clone, Default, Deref, DerefMut)]
pub struct IdRanges(Vec<IdRangeMap>);

impl IdRanges {
    pub fn write_to(&self, mut w: impl Write) -> Result<()> {
        w.write_all(self.serialize().as_bytes())?;
        Ok(())
    }

    pub fn serialize(&self) -> String {
        self.0
            .iter()
            .map(IdRangeMap::serialize)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Deref)]
pub struct ChildProcess(nix::unistd::Pid);

impl ContainerHandle for ChildProcess {
    fn pid(&self) -> u32 {
        self.0.as_raw().try_into().unwrap()
    }

    fn wait(&mut self) -> Result<i32> {
        match nix::sys::wait::waitpid(self.0, None)? {
            nix::sys::wait::WaitStatus::Exited(_, status) => Ok(status),
            r => Err(anyhow::anyhow!(
                "Child process did not exit normally: {r:?}"
            )),
        }
    }
}

#[derive(Debug, Builder)]
#[builder(build_fn(name = "build", vis = ""))]
pub struct UnshareEnvironment {
    #[builder(default, setter(custom, name = "namespace"))]
    namespaces: Vec<UnshareNamespaces>,
    #[builder(default, setter(custom, name = "uid_map"))]
    uid_maps: IdRanges,
    #[builder(default, setter(custom, name = "gid_map"))]
    gid_maps: IdRanges,
    #[builder(default)]
    fork: bool,
    #[builder(default, setter(strip_option, into))]
    root: Option<PathBuf>,
}

#[allow(dead_code)]
impl UnshareEnvironmentBuilder {
    pub fn uid_map(&mut self, uid_map: IdRangeMap) -> &mut Self {
        self.uid_maps
            .get_or_insert_with(Default::default)
            .push(uid_map);
        self
    }

    pub fn gid_map(&mut self, gid_map: IdRangeMap) -> &mut Self {
        self.gid_maps
            .get_or_insert_with(Default::default)
            .push(gid_map);
        self
    }

    pub fn namespace(&mut self, namespace: UnshareNamespaces) -> &mut Self {
        self.namespaces
            .get_or_insert_with(std::vec::Vec::new)
            .push(namespace);
        self
    }

    pub fn map_current_user_to_root(&mut self) -> &mut Self {
        let mapping = IdRangeMap {
            outer_id_start: nix::unistd::getuid().into(),
            inner_id_start: 0,
            count: 1,
        };
        self.uid_map(mapping.clone());
        self.gid_map(mapping);
        self
    }

    #[instrument(level = "trace", skip_all, err(level = Level::TRACE))]
    pub fn enter(self) -> Result<Option<ChildProcess>> {
        let unshare = self.build().context("Building unshare options")?;
        // if !unshare.uid_map.is_empty() || !unshare.gid_map.is_empty() {
        //     std::fs::write("/proc/self/setgroups", "deny").context("Disallowing setgroups")?;
        //     write_mappings("/proc/self/uid_map", &unshare.uid_map).context("Writing uid map")?;
        //     write_mappings("/proc/self/gid_map", &unshare.gid_map).context("Writing gid map")?;
        // }

        let clone_flags = unshare
            .namespaces
            .into_iter()
            .fold(nix::sched::CloneFlags::empty(), |flags, namespace| {
                flags.union(namespace.into())
            });
        nix::sched::unshare(clone_flags).context("Entering new namespace")?;

        if !unshare.uid_maps.is_empty() || !unshare.gid_maps.is_empty() {
            std::fs::write("/proc/self/setgroups", "deny").context("Disallowing setgroups")?;
            write_mappings("/proc/self/uid_map", &unshare.uid_maps).context("Writing uid map")?;
            write_mappings("/proc/self/gid_map", &unshare.gid_maps).context("Writing gid map")?;
        }

        if unshare.fork {
            if let nix::unistd::ForkResult::Parent { child } = unsafe { nix::unistd::fork() }? {
                return Ok(Some(ChildProcess(child)));
            }
        }

        if let Some(root) = unshare.root {
            nix::unistd::chroot(&root)
                .with_context(|| format!("Chrooting to {}", root.display()))?;
            nix::unistd::chdir("/").with_context(|| "Changing directory to /".to_string())?;
        }
        Ok(None)
    }
}

fn write_mappings(p: impl AsRef<Path>, mappings: &IdRanges) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(p)
        .context("Opening mapping file")?;

    mappings.write_to(&mut file).context("Writing mapping")?;
    Ok(())
}
