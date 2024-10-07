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
            simple-http-server
          ];
          entryPoint = ''
            echo -e "\n# Mounts"
            mount
            echo -e "\n# Environment"
            env
            echo -e "\n# Network"
            ip addr
            echo -e "\n# ls ''${1:-/}"
            ls -alh ''${1:-/}
            exec simple-http-server --port 8080
          '';
        };
      }
    );
}
