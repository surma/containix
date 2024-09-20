{
  iana-etc,
  util-linux,
  iproute2,
  coreutils,
  bash,
  buildEnv,
}@args:
buildEnv {
  name = "containix-base";
  paths = builtins.removeAttrs args [
    "buildEnv"
    "lib"
  ];
}
