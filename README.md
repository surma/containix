# Containix

Containix is a replacement(-ish) for Docker, using [Nix Flakes] as the container specification.

By running `containix run -f /path/to/flake`, Containix will build the default package of the flake, determine the transitive closure of dependencies, and create a container with a read-only nix store with _only_ those dependencies. If no command is specified, `containix-entry-point` will be run, otherwise the specified command and arguments will be used.

Take a look at the [examples](./examples/) to get started.

## Installation

You must have nix installed for containix to work. Therefore, you might as well run containix via nix:

```console
$ nix run github:surma/containix
```

## Usage

Run a flake inside a container:

```console
$ containix run -f /path/to/flake
```

Run MySQL inside a container:

```console
$ containix run -f 'github:nixos/nixpkgs/24.05#mysql' -- mysql --version
```

Mount a host directory into the container:

```console
$ containix run -f /path/to/flake --volume $PWD:/workdir
```

Since containers are flakes, you can run the examples from this repository as follows:

```console
$ containix -f 'github:surma/containix?dir=examples/simple_container'
```
