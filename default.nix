{
  rustPlatform,
  lib,
  system
}:
let
  toml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = toml.package.name;
  version = toml.package.version;
  src = lib.sources.sourceByRegex ./. [
    "Cargo\.(lock|toml)"
    "src(/.*\.rs)?"
  ];
  cargoLock = {
    lockFile = ./Cargo.lock;
  };
  doCheck = false;
}
