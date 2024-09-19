# Containix

Containix is a replacement(-ish) for Docker, using [Nix Flakes] as the container specification.

By running `containix -f /path/to/flake`, Containix will build the default package of the flake, determine the transitive closure of dependencies, and create a container with a read-only nix store with _only_ those dependencies. Then, similar to `nix run`, it will execute the binary with the same name as the default package’s derivation name.

Take a look at the [examples](./example/) to get started.

> NB: If you use `--network`, Containix requires the `ip` tool from the `iproute2` package to be present inside the container.

## Installation

1. Clone the repository:

   ```console
   $ git clone https://github.com/surma/containix
   $ cd containix
   ```

2. Build the project:

   ```console
   $ nix build .
   ```

   If you don’t have Nix installed, you can use bog-standard cargo, but make sure to target your architecture wuth the muslibc to ensure a statically linked binary:

   ```console
   $ cargo build --target x86_64-unknown-linux-musl --release
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
