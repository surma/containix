{
  iana-etc,
  util-linux,
  iproute2,
  coreutils,
  bash,
  buildEnv,
  lib
}@args:
buildEnv {
  name = "containix-base";
  paths = lib.removeAttrs args [
    "buildEnv"
    "lib"
  ];
}
