# Containix

Containix is a containerization tool that relies on [Nix] to build the container environment rather than full-blown Linux system file trees.

Containix uses [Nix Flakes] as the container specification format. Take a look at the [examples] to get an impression for what that looks like.

## Installation

You must have Nix installed for containix to work. Therefore, you can run containix directly via nix:

```console
$ nix run github:surma/containix
```

## Usage

Run a flake inside a container:

```console
$ containix -f /path/to/flake
```

Since any flake expression can be used for the container, you can run the examples from this repository:

```console
$ containix -f 'github:surma/containix?dir=examples/simple_container'
```

Mount a host directory as read-only into the container, set an environment variable, expose a port and bind a webserver to it:

```console
$ containix -f 'github:surma/containix?dir=examples/webserver' \
    --volume $PWD:/var/www:ro \
    --port 8080:8123 \
    --env PORT=8123
```

Write your own container flake:

```console
nix flake init -t github:surma/containix
# ... edit flake.nix ...
containix -f .
```

[Nix]: https://nixos.org/
[Nix Flakes]: https://nixos.wiki/wiki/Flakes
[examples]: ./examples/