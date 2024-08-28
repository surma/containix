use std::collections::HashSet;

use anyhow::Result;

use crate::unix_helpers::str_as_array;

pub fn network_interface_names() -> Result<Vec<String>> {
    let interfaces: HashSet<_> = nix::ifaddrs::getifaddrs()?
        .map(|i| i.interface_name)
        .collect();
    Ok(Vec::from_iter(interfaces))
}

pub fn set_ip_address(
    interface: impl AsRef<str>,
    ip_address: std::net::Ipv4Addr,
    netmask: std::net::Ipv4Addr,
) -> Result<()> {
    use libc::*;

    unsafe {
        let fd = socket(AF_INET, SOCK_DGRAM, 0);
        if fd < 0 {
            anyhow::bail!("Failed to open socket");
        }
        let _guard = scopeguard::guard(fd, |fd| {
            close(fd);
        });

        let mut ifr = ifreq {
            ifr_ifru: std::mem::zeroed(),
            ifr_name: str_as_array(interface.as_ref()),
        };
        ifr.ifr_ifru.ifru_addr.sa_family = AF_INET.try_into().unwrap();

        store_ip_address(&mut ifr, ip_address);
        if (ioctl(fd, SIOCSIFADDR.try_into().unwrap(), &ifr) < 0) {
            anyhow::bail!(
                "Failed to set {} to address {}",
                interface.as_ref(),
                ip_address.to_string()
            );
        }

        store_ip_address(&mut ifr, netmask);
        if (ioctl(fd, SIOCSIFNETMASK.try_into().unwrap(), &ifr) < 0) {
            anyhow::bail!(
                "Failed to set {} to netmask {}",
                interface.as_ref(),
                netmask.to_string()
            );
        }

        ifr.ifr_ifru.ifru_flags |= <i32 as TryInto<i16>>::try_into(IFF_UP | IFF_RUNNING).unwrap();
        if (ioctl(fd, SIOCSIFFLAGS.try_into().unwrap(), &ifr) < 0) {
            anyhow::bail!("Failed to bring {} up", interface.as_ref(),);
        }
    }

    Ok(())
}

unsafe fn store_ip_address(ifr: &mut libc::ifreq, ip_address: std::net::Ipv4Addr) {
    // Honestly, no idea why it has to be 2..6, but this is what the internet says to do.
    ifr.ifr_ifru.ifru_addr.sa_data[2..6]
        .copy_from_slice(std::mem::transmute(ip_address.octets().as_slice()));
}
