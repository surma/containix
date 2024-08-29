use crate::nix_helpers::Nixpkgs;
use std::ffi::OsString;
use std::{str::FromStr, sync::LazyLock};

pub fn is_container() -> bool {
    std::env::var("CONTAINIX_CONTAINER").is_ok()
}

pub const NIXPKGS_24_05: &str = "https://github.com/NixOS/nixpkgs/archive/refs/tags/24.05.tar.gz?sha256=1lr1h35prqkd1mkmzriwlpvxcb34kmhc9dnr48gkm8hh089hifmx";

macro_rules! tool {
    ($name:ident, $component:expr, $bin:literal) => {
        pub static $name: LazyLock<OsString> = LazyLock::new(|| {
            let cmd = if is_container() {
                $bin.into()
            } else {
                NIXPKGS
                    .realise($component)
                    .expect("Nixpkgs must provide $component")
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

pub static NIXPKGS: LazyLock<Nixpkgs> = LazyLock::new(|| {
    Nixpkgs::from_str(NIXPKGS_24_05).expect("Hard-coded Nixpkgs URL must be valid")
});

tool!(MOUNT, "util-linux", "mount");
tool!(UMOUNT, "util-linux", "umount");
tool!(IP, "iproute2", "ip");
tool!(UNSHARE, "util-linux", "unshare");
