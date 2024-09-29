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
    paths = with pkgs; [
      iana-etc
      # `mount`, `umount`, `more`, etc
      util-linux
      # `ls`, `cat`, `cp`, etc
      coreutils
      # `bash`
      bash
    ];
  };

  # mkInitScript = entryPoint:
  # let
  #   inherit (pkgs) writeShellScriptBin;
  # in

  buildContainerEnv =
    {
      entryPoint,
      packages ? [ ],
      fs ? defaultFs,
    }:
    let
      inherit (pkgs) writeShellScriptBin rsync coreutils;

      packagEnv = buildEnv {
        name = "container-env";
        paths = packages;
      };
    in
    writeShellScriptBin "containix-entry-point" ''
      PATH=${rsync}/bin:${coreutils}/bin

      mkdir -p /var/{lib,log,run,cache,lock,tmp} /tmp/containix
      rsync -T /tmp/containix -rL ${fs}/ /

      export PATH=${packagEnv}/bin
      exec ${writeShellScriptBin "containix-entry-point" entryPoint}/bin/containix-entry-point
    '';
in
# buildEnv {
#   name = "container-fs";
#   paths = [ (mkInitScript entryPoint) ] ++ basePackages ++ extraPackages;
# };
{
  inherit buildContainerEnv;
}
