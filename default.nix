{
  pkgs ? import <nixpkgs> { },
}:
let
  inherit (pkgs) rustPlatform;
  toml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = toml.package.name;
  version = toml.package.version;
  # src = pkgs.symlinkJoin [./src ./Cargo.toml ./Cargo.lock];
  src = ./.;
  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = with pkgs; [ pkg-config ];

  buildInputs = with pkgs; [
    # Add any system dependencies here
  ];

  # If you have any post-installation steps, add them here
  # postInstall = ''
  #   # Your post-install commands
  # '';
}
