{
  description = "Simple container";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.05";
    flake-utils.url = "github:numtide/flake-utils";
    # In your own container flakes, you should 
    # reference the repository directly. I.e:
    # ```
    # recontainix.url = "github:surma/containix";
    # ```
    # For development and testing purposes, the path here will remain relative.
    containix.url = "../../";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      containix,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs)
          mkShell
          buildEnv
          writeShellScriptBin
          callPackage
          ;
      in
      rec {
        packages.default = buildEnv {
          name = "simple-container";
          paths = [
            (writeShellScriptBin "simple-container" ''
              echo -e "\n# Mounts"
              mount
              echo -e "\n# Environment"
              env
              echo -e "\n# Network"
              ip addr
              echo -e "\n# ls ''${1:-/}"
              ls -alh ''${1:-/}
              exec /bin/bash
            '')
            containix.packages.${system}.base
          ];
        };
        apps.default = flake-utils.lib.mkApp { drv = packages.default; };
        devShell = mkShell {
          buildInputs = with pkgs; [
            nix
            rustc
            cargo
            pkg-config
            glibc
          ];
        };
      }
    );
}
