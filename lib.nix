{
  writeShellScriptBin,
  buildEnv,
  pkgs,
  coreutils,
  rsync,
  util-linux,
  lib,
}:
let
  defaultFs = buildEnv {
    name = "container-fs";
    paths = with pkgs; [ iana-etc ];
  };

  buildContainerEnv =
    {
      entryPoint,
      envs ? { },
      packages ? [ ],
      fs ? defaultFs,
    }:
    let
      packageEnv = buildEnv {
        name = "container-env";
        paths = packages;
      };

      env = ({
        HOME = "/root";
        PATH = "${packageEnv}/bin";
      }) // envs;

      env_setup = lib.strings.concatLines (
        lib.attrsets.mapAttrsToList (name: value: "export ${name}=${value}") env
      );
    in
    writeShellScriptBin "containix-entry-point" ''
      PATH=${rsync}/bin:${util-linux}/bin:${coreutils}/bin

      ${if (fs != null) then "rsync -rL ${fs}/ /" else ""}

      mkdir /proc
      mount -t proc proc /proc

      ${env_setup}
      exec ${writeShellScriptBin "containix-entry-point" entryPoint}/bin/containix-entry-point
    '';
in
{
  inherit buildContainerEnv;
}
