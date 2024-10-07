{
  description = "Basic Containix Container";
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
          ];
          entryPoint = ''
            exec bash
          '';
        };
      }
    );
}
