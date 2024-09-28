{ lib, crate2nix }:
let
  toml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
  src = lib.sources.sourceByRegex ./. [
    "Cargo\.(lock|toml)"
    "src(/.*\.rs)?"
  ];

  cargoNix = crate2nix.appliedCargoNix {
    name = toml.package.name;
    inherit src;
  };
in
cargoNix.rootCrate.build
