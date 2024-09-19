{
  description = "containix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/a6292e34000dc93d43bccf78338770c1c5ec8a99";
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
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages = {
          default = import ./default.nix {
            inherit pkgs system;
            fenix = fenix.packages.${system};
          };
        };
      }
    );
}
