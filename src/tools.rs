use crate::nix_helpers::NixDerivation;
use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::LazyLock;

pub fn is_container() -> bool {
    std::env::var("CONTAINIX_CONTAINER").is_ok()
}

pub const NIXPKGS: &str = "github:nixos/nixpkgs/24.05";

pub struct Tool {
    pub component: String,
    pub bin: String,
    pub path: OsString,
}

macro_rules! tools {
    {$(($component:expr, $bin:literal)),*} => {
        pub static TOOLS: LazyLock<HashMap<String, Tool>> = LazyLock::new(|| {
            HashMap::from([
                $(
                    {
                        let path = if is_container() {
                            $bin.into()
                        } else {
                            NixDerivation::package_from_flake($component, NIXPKGS)
                                .build()
                                .expect(&format!("Nixpkgs must provide {}", $component))
                                .as_path()
                                .join("bin")
                                .join($bin)
                                .as_os_str()
                                .to_os_string()
                        };
                        tracing::trace!(
                            r#"Using "{}" as {}"#,
                            path.to_string_lossy(),
                            $bin
                        );
                        (($bin).to_string(), Tool {
                            component: $component.to_string(),
                            bin: $bin.to_string(),
                            path
                        })
                    }
                ),*
            ])
        });
    };
}

tools! {
    ("util-linux", "mount"),
    ("util-linux", "umount"),
    ("iproute2", "ip"),
    ("util-linux", "unshare")
}
