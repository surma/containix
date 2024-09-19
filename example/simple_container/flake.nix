{
  description = "Simple container";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.05";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs) mkShell buildEnv writeShellScriptBin;
      in
      rec {
        packages.default = buildEnv {
          name = "simple-container";
          paths =
            [
              (writeShellScriptBin "simple-container" ''
                echo -e "\n# Environment"
                env
                echo -e "\n# Mounts"
                mount
                echo -e "\n# Network"
                ip addr
                echo -e "\n# ls ''${1:-/}"
                ls -alh ''${1:-/}
              '')
            ]
            ++ (with pkgs; [
              coreutils
              iproute2
              util-linux
            ]);
        };
      }
    );
}
