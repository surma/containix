{
  description = "containix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.05";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    crate2nix.url = "github:nix-community/crate2nix";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
      crate2nix,
    }:
    let
      inherit (flake-utils.lib) eachSystem system;
      linuxSystems = builtins.filter (s: builtins.match ".*linux.*" s != null) (
        builtins.attrValues system
      );
    in
    eachSystem linuxSystems (
      system:
      let
        toolchain = fenix.packages.${system}.stable.defaultToolchain;
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (final: prev: {
              rustc = toolchain;
              cargo = toolchain;
            })
          ];
        };
        inherit (pkgs) callPackage buildEnv;

        crate2nix' = callPackage (import "${crate2nix}/tools.nix") { };
      in
      rec {
        packages = {
          default = packages.containix;
          containix = callPackage (import ./default.nix) { crate2nix = crate2nix'; };
          container-tools = callPackage (import ./container-tools.nix) { };
        };

        lib = {
          containerFS =
            {
              extraPackages ? [ ],
            }:
            buildEnv {
              name = "container-fs";
              paths = extraPackages ++ [
                packages.container-tools
                packages.containix
              ];
            };
        };
      }
    );
}
