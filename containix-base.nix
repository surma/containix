{
  # `/etc/protocols` etc (e.g. `ping` needs this to work)
  iana-etc,
  # `mount`, `umount`, `more`, etc
  util-linux,
  # `ip`
  iproute2,
  # `ls`, `cat`, `cp`, etc
  coreutils,
  # `bash`
  bash,

  # Stuff required to assemble the base layer.
  buildEnv,
  lib,
}@args:
let
  inherit (lib.attrsets) removeAttrs attrValues;
in
buildEnv {
  name = "containix-base";
  # Slightly level of indirection. But this way adding an input to this callPackage pattern
  # will make it automatically be part of the base.
  paths = attrValues (
    removeAttrs args [
      "buildEnv"
      "lib"
    ]
  );
}
