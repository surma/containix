{
  description = "containix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.05";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
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
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs) callPackage;
      in
      {
        packages = rec {
          default = containix;
          containix = callPackage (import ./default.nix) {
            rustPlatform = pkgs.makeRustPlatform fenix.packages.${system}.stable;
          };
          base = callPackage (import ./containix-base.nix) { inherit containix; };
        };
      }
    );
}
