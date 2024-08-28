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
   $ cargo build --release
   ```

   Or

   ```console
   $ nix-build
   ```

## Usage

```console
$ containix create-container --volume $HOME:/root --expose bash --expose coreutils bash
```
