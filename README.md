# Containix

Containix is a containerization tool that relies on [Nix] to define and build the container environment rather than using images of Linux system file trees like [Docker] does.

Containix uses [Nix Flakes] as the container definition format. Take a look at the [examples] to get an impression for what that looks like.

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

This will build the flake’s `containix` package if it exists, otherwise it will use the `default` package.

Since any flake expression can be used for the container, you can run the examples from this repository:

```console
$ containix -f 'github:surma/containix?dir=examples/simple_container'
```

Many of the familiar flags from [Docker] are supported: `-v` mounts a host directory into the container, `-e` set an environment variable and `-e` expose a port:

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
[Docker]: https://www.docker.com/

---

Apache License 2.0
