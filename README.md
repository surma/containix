# Containix

Containix is a replacement(-ish) for Docker, using [Nix Flakes] as the container specification.

By running `containix -f /path/to/flake`, Containix will build the default package of the flake, determine the transitive closure of dependencies, and create a container with a read-only nix store with _only_ those dependencies. Then, similar to `nix run`, it will execute the binary with the same name as the default package’s derivation name.

Take a look at the [examples](./example/) to get started.

> NB: If you use `--network`, Containix requires the `ip` tool from the `iproute2` package to be present inside the container.

## Installation

You must have nix installed for containix to work. As a result, you can run containix directly:

```console
$ nix run github:surma/containix
```

## Usage

Run a flake inside a container:

```console
$ containix -f /path/to/flake
```

Mount a host directory into the container and create a subnet where 10.0.0.1 is the host and 10.0.0.2 is the container:

```console
$ containix -f /path/to/flake --volume $PWD:/workdir --network 10.0.0.1+10.0.0.2/8
```

Since containers are flakes, you can run the examples in this repository as follows:

```console
$ containix -f 'github:surma/containix?dir=example/simple_container'
```

## Development Building

For development, you can use normal cargo. However, make sure to target your architecture with the muslibc to get a statically linked binary:

```console
$ cargo build --target x86_64-unknown-linux-musl --release
```
