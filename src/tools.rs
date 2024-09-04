use crate::nix_helpers::NixBuild;
use std::ffi::OsString;
use std::{str::FromStr, sync::LazyLock};

pub fn is_container() -> bool {
    std::env::var("CONTAINIX_CONTAINER").is_ok()
}

pub const NIXPKGS_24_05: &str = "git+https://github.com/nixos/nixpkgs?tag=24.05";

macro_rules! tool {
    ($name:ident, $component:expr, $bin:literal) => {
        pub static $name: LazyLock<OsString> = LazyLock::new(|| {
            let cmd = if is_container() {
                $bin.into()
            } else {
                NixBuild::nixpkg_component($component, NIXPKGS_24_05)
                    .build()
                    .expect("Nixpkgs must provide $component")
                    .as_path()
                    .join("bin")
                    .join($bin)
                    .as_os_str()
                    .to_os_string()
            };
            tracing::trace!(
                r#"Using "{}" as {}"#,
                cmd.to_string_lossy(),
                stringify!($bin)
            );
            cmd
        });
    };
}

tool!(MOUNT, "util-linux", "mount");
tool!(UMOUNT, "util-linux", "umount");
tool!(IP, "iproute2", "ip");
tool!(UNSHARE, "util-linux", "unshare");
