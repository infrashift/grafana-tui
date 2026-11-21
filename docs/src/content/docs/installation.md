---
title: Installation
description: Install grafana-tui into ~/.local/bin or another directory on your PATH.
---

## Installer Script

Install the latest prebuilt release. This method does not need Rust:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/infrashift/grafana-tui/main/install.sh | bash'
```

With `wget`:

```bash
bash -o pipefail -c 'wget -O- https://raw.githubusercontent.com/infrashift/grafana-tui/main/install.sh | bash'
```

The script supports Linux and macOS on x86_64 and ARM64. It installs to
`$HOME/.local/bin` and never invokes `sudo`. Make sure that this directory is
on the `PATH`.

To install into another directory, set `GRAFANA_TUI_INSTALL_DIR`. The directory
must be on the `PATH`, and you must have write access to it:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/infrashift/grafana-tui/main/install.sh | GRAFANA_TUI_INSTALL_DIR=$HOME/bin bash'
```

To install a specific release, set `GRAFANA_TUI_VERSION`. The `v` at the start
is optional:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/infrashift/grafana-tui/main/install.sh | GRAFANA_TUI_VERSION=v0.1.11 bash'
```

The script verifies each release download against its published SHA-256
checksum manifest. If the manifest is not available, or if the check fails, the
script stops.

We recommend that you read a downloaded script before you run it:

```bash
curl --proto '=https' --tlsv1.2 -LsSf -o install.sh https://raw.githubusercontent.com/infrashift/grafana-tui/main/install.sh
less install.sh
bash install.sh
```

## Prebuilt Archive

Each [GitHub Release](https://github.com/infrashift/grafana-tui/releases)
publishes a `.tar.gz` archive for Linux and macOS on x86_64 and ARM64, plus
`grafana-tui-checksums.txt` with their SHA-256 hashes. To install one by hand:

1. Download the archive for the platform.
2. Download the checksum manifest.
3. Verify the archive against the manifest.
4. Extract the binary into `~/.local/bin`.

```bash
asset=grafana-tui-x86_64-unknown-linux-gnu.tar.gz
base=https://github.com/infrashift/grafana-tui/releases/latest/download
curl --proto '=https' --tlsv1.2 -LsSfO "${base}/${asset}"
curl --proto '=https' --tlsv1.2 -LsSfO "${base}/grafana-tui-checksums.txt"
sha256sum --check --ignore-missing grafana-tui-checksums.txt
mkdir -p ~/.local/bin
tar -xzf "${asset}" -C ~/.local/bin grafana-tui
```

On macOS, use `shasum -a 256 --check --ignore-missing` instead of `sha256sum`.

On Windows, download `grafana-tui-x86_64-pc-windows-msvc.zip` and extract
`grafana-tui.exe` into a directory on the `PATH`.

## From Source

grafana-tui requires Rust 1.88 or newer.

1. Clone the repository.
2. Install the binary into `~/.local/bin`.

```bash
git clone https://github.com/infrashift/grafana-tui.git
cd grafana-tui
cargo install --path . --root ~/.local
```

To run a build from the source tree, use `cargo run`:

```bash
cargo run -- --prometheus-url http://localhost:9090
```

## Making Sure It Is on PATH

Each method above installs one self-contained binary. We do not publish
grafana-tui on crates.io or in a third-party package manager. If the shell
cannot find `grafana-tui`, add the install directory to the `PATH`. For
example, add this line to `~/.bashrc` or `~/.zshrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

To uninstall, delete the binary from the directory you installed it into.

## Shell Completions

grafana-tui can generate shell completions for Bash, Zsh, Fish, PowerShell, and Elvish.

```bash
# Bash
source <(grafana-tui completions bash)

# Zsh
source <(grafana-tui completions zsh)

# Fish
grafana-tui completions fish | source
```

## Man Page

Generate a man page from the CLI definition:

```bash
grafana-tui man > grafana-tui.1
```
