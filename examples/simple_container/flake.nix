{
  description = "Simple container printing some info and dropping into a shell";
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
        inherit (containix.lib.${system}) buildContainerEnv;
      in
      rec {
        packages.default = buildContainerEnv {
          packages = with pkgs; [
            bash
            coreutils
            util-linux
            inetutils
            shadow
            su
            iproute2
          ];
          entryPoint = ''
            echo -e "\n# Mounts"
            mount
            echo -e "\n# Environment"
            env
            echo -e "\n# Network"
            ip addr
            exec bash
          '';
        };
      }
    );
}
