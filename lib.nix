{
  writeShellScriptBin,
  buildEnv,
  pkgs,
}:
{
  containerFS =
    {
      extraPackages ? [ ],
      entryPoint ? null,
      basePackages ? (
        with pkgs;
        [
          # `/etc/protocols` etc (e.g. `ping` needs this to work)
          iana-etc
          # `mount`, `umount`, `more`, etc
          util-linux
          # `ls`, `cat`, `cp`, etc
          coreutils
          # `bash`
          bash
        ]
      ),
    }:
    let
      entryPointScript = (writeShellScriptBin "containix-entry-point" entryPoint);
    in
    buildEnv {
      name = "container-fs";
      paths = [ entryPointScript ] ++ basePackages ++ extraPackages;
    };
}
