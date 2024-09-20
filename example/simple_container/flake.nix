{
  description = "Simple container";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.05";
    flake-utils.url = "github:numtide/flake-utils";
    containix.url = "github:surma/containix";
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
              echo -e "\n# Environment"
              env
              # echo -e "\n# Mounts"
              # mount
              echo -e "\n# Network"
              ip addr
              echo -e "\n# ls ''${1:-/}"
              ls -alh ''${1:-/}
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
