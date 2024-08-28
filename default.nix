{
  pkgs ? import <nixpkgs> { },
  fenix ? pkgs.callPackage (import (
    fetchTarball "https://github.com/nix-community/fenix/archive/main.tar.gz"
  )) { },
  system ? builtins.currentSystem,
}:
let
  buildRustPackage = pkgs.callPackage ./build-rust-package.nix { inherit fenix; };

  toml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
  muslTarget = pkgs.lib.strings.replaceStrings [ "-gnu" ] [
    "-musl"
  ] pkgs.stdenv.hostPlatform.rust.rustcTargetSpec;
in
buildRustPackage {
  pname = toml.package.name;
  version = toml.package.version;
  src = ./.;
  cargoLock = {
    lockFile = ./Cargo.lock;
  };
  target = muslTarget;
}
