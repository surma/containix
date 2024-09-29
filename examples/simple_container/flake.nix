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
        inherit (pkgs) writeShellScriptBin;
      in
      rec {
        packages.default = writeShellScriptBin "containix-entry-point" ''
          PATH=${pkgs.coreutils}/bin:${pkgs.util-linux}/bin

          echo -e "\n# Mounts"
          mount
          echo -e "\n# Environment"
          env
          echo -e "\n# ls ''${1:-/}"
          ls -alh ''${1:-/}
          exec ${pkgs.bash}/bin/bash
        '';
      }
    );
}
