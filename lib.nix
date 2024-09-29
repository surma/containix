{
  writeShellScriptBin,
  buildEnv,
  pkgs,
  coreutils,
  bash,
  system,
}:
let
  inherit (pkgs) buildEnv;
  defaultFs = buildEnv {
    name = "container-fs";
    paths = with pkgs; [ iana-etc ];
  };

  buildContainerEnv =
    {
      entryPoint,
      packages ? [ ],
      fs ? defaultFs,
    }:
    let
      inherit (pkgs)
        writeShellScriptBin
        rsync
        coreutil
        util-linux
        ;

      packagEnv = buildEnv {
        name = "container-env";
        paths = packages;
      };
    in
    writeShellScriptBin "containix-entry-point" ''
      PATH=${rsync}/bin:${util-linux}/bin:${coreutils}/bin

      test -n "${fs}" &&rsync -rL ${fs}/ /

      mount -t proc proc /proc

      export PATH=${packagEnv}/bin
      exec ${writeShellScriptBin "containix-entry-point" entryPoint}/bin/containix-entry-point
    '';
in
{
  inherit buildContainerEnv;
}
