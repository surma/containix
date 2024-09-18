# Containix

Containix is a lightweight approach to containers by relying on Nix and the Nix store to handle the container's filesystem.

## Features

- Create ephemeral containers with specified Nix component
- Automatic cleanup of ephemeral container resources
- Mount volumes into containers
- (TODO) Easy network interface configuration
- (TODO) Port mapping

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

```console
$ containix create-container --volume $HOME:/root --package bash --package coreutils bash
```
