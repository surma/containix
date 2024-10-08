{
  writeShellScriptBin,
  writeTextFile,
  buildEnv,
  pkgs,
  coreutils,
  rsync,
  util-linux,
  lib,
}:
let
  resolvConf = writeTextFile {
    name = "resolve.conf";
    text = ''
      nameserver 8.8.8.8
    '';
    executable = false;
    destination = "/etc/resolv.conf";
  };
  defaultFs = buildEnv {
    name = "container-fs";
    paths = with pkgs; [
      iana-etc
      resolvConf
    ];
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

      env_setup = lib.strings.concatLines (
        lib.attrsets.mapAttrsToList (name: value: "export ${name}=${value}") envs
      );
    in
    writeShellScriptBin "containix-entry-point" ''
      PATH=${rsync}/bin:${util-linux}/bin:${coreutils}/bin

      ${if (fs != null) then "rsync -rL ${fs}/ /" else ""}

      mkdir /proc
      mount -t proc proc /proc

      echo root:x:0:0:root:/root:/bin/bash >> /etc/passwd
      echo root:x:0: >> /etc/group
      echo root:*:19908:0:99999:7::: >> /etc/shadow

      export PATH=${packageEnv}/bin
      ${env_setup}
      exec ${writeShellScriptBin "containix-entry-point" entryPoint}/bin/containix-entry-point
    '';
in
{
  inherit buildContainerEnv;
}
