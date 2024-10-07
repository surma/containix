{
  description = "Spawns a web server on port $PORT (default: 8080), serving /var/www";
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
          packages = with pkgs; [ simple-http-server ];
          entryPoint = ''
            # This looks a bit odd, but we have to prevent nix from interpolating the string.
            exec simple-http-server --port ${"$"}{PORT:-8080} /var/www 
          '';
        };
      }
    );
}
