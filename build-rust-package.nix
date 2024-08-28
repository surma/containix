{
  system,
  fenix,
  rustPlatform,
  stdenv,
  jq,
  rsync,
}:

{
  target,
  src,
  cargoLock ? {
    lockFile = "${src}/Cargo.lock";
  },
  release ? true,
  fhsName ? "bin",
  ...
}@args:
let
  toolchain =
    with fenix.packages.${system};
    fenix.combine [
      fenix.stable.rustc
      fenix.stable.cargo
      fenix.targets.${target}.stable.rust-std
    ];

  # Create `vendor` folder with all dependencies.
  vendoredDependencies = rustPlatform.importCargoLock cargoLock;

  rustcTargetDir = "target/${target}/${if release then "release" else "debug"}";
in
stdenv.mkDerivation (
  (removeAttrs args [ "cargoLock" ])
  // {
    nativeBuildInputs = (args.nativeBuildInputs or [ ]) ++ [
      toolchain
      jq
      rsync
    ];
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild

      cargo build \
        --config 'source.crates-io.replace-with="vendored-sources"' \
        --config 'source.vendored-sources.directory="${vendoredDependencies}"' \
        --offline \
        --target ${target} ${if release then "-r" else ""}

      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall;
      mkdir -p $out/${fhsName}

      find ${rustcTargetDir} -type f -maxdepth 1 | \
        xargs -I ___X -n1 cp ___X $out/${fhsName}

      runHook postInstall;
    '';
  }
)
