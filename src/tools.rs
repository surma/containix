use std::{path::PathBuf, str::FromStr, sync::LazyLock};

use crate::nix_helpers::Nixpkgs;

pub const NIXPKGS_24_05: &str = "https://github.com/NixOS/nixpkgs/archive/refs/tags/24.05.tar.gz?sha256=1lr1h35prqkd1mkmzriwlpvxcb34kmhc9dnr48gkm8hh089hifmx";

pub static NIXPKGS: LazyLock<Nixpkgs> = LazyLock::new(|| {
    Nixpkgs::from_str(NIXPKGS_24_05).expect("Hard-coded Nixpkgs URL must be valid")
});

pub static UTIL_COMPONENT: LazyLock<PathBuf> = LazyLock::new(|| {
    NIXPKGS
        .realise("util-linux")
        .expect("Nixpkgs must provide util-linux")
});
