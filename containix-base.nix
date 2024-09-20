{
  iana-etc,
  util-linux,
  iproute2,
  coreutils,
  bash,
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
