# Dail
Daily jail management for FreeBSD

## Features

- **Familiar workflow:** `create`, `start`, `stop`, `rm`, `run`, `exec`, `build`
- **Dailfile:** declarative jail builds
- **Presets:** one flag to configure common workloads (`--preset postgres`)
- **Thick & thin jails:** full copy or shared base with per-jail overlay
- **Networking:** inherit, IP alias, or VNET with bridge
- **ZFS support:** snapshots, clones, ZFS-backed storage
- **Resource limits:** rctl-based CPU/memory/process limits

## Quick Start

```bash
# Initialize dail
dail config init

# Download FreeBSD base (default: 15.0-RELEASE)
dail bootstrap

# Run a jail (create + start in one step)
dail run myjail

# Run with a preset
dail run postgres-jail --preset postgres

# Run a disposable jail (auto-removed on stop)
dail run temp-jail --rm
```

## Commands

### Setup

**`dail config init`** — Initialize dail: create directory structure (`/var/db/dail/`), write default config.

```bash
dail config init                    # directory backend
dail config init --zfs-pool zroot   # ZFS backend
```

**`dail bootstrap`** — Download and extract a FreeBSD base system.

```bash
dail bootstrap                      # download default (15.0-RELEASE)
dail bootstrap 14.2-RELEASE         # specific release
dail bootstrap --list               # show bootstrapped bases
```

### Jail Lifecycle

**`dail create`** — Create a jail without starting it.

```bash
dail create myjail                                          # thick jail, default base
dail create myjail --type thin --base 14.2-RELEASE
dail create myjail --preset postgres                        # apply preset
dail create web --vnet --vnet-ip 10.0.0.5/24 --vnet-gateway 10.0.0.1
dail create app --mount /data/app:/app --allow raw_sockets --limit maxproc=256
```

**`dail run`** — Create and start a jail in one step. Same options as `create`, plus `--rm`, `--build`, `--rebuild`, `--image`.

```bash
dail run myjail                                             # create + start
dail run postgres-jail --preset postgres                    # with preset
dail run temp --rm                                          # auto-remove on stop
dail run web --vnet --vnet-ip 10.0.0.5/24 --vnet-gateway 10.0.0.1
dail run app --mount /data:/app --preset dev --limit maxproc=512
dail run postgres-jail --build examples/postgres/Dailfile # build + start
dail run postgres-jail --build examples/postgres/Dailfile --rebuild  # rebuild from scratch
dail run pg --image postgres:18                             # run from saved image
dail run pg --image postgres:18 --ip 10.100.0.50 --persist  # with overrides
```

**`dail start`** / **`stop`** / **`restart`** — Manage jail state.

```bash
dail start myjail
dail stop myjail                    # if --rm was set, jail is auto-removed
dail restart myjail
```

**`dail rm`** — Remove a jail and its filesystem.

```bash
dail rm myjail                      # must be stopped
dail rm myjail --force              # stop + remove
```

### Inspection

**`dail ls`** — List jails.

```bash
dail ls                             # all jails
dail ls --running                   # only running
dail ls --format json               # JSON output
```

**`dail inspect`** — Show full jail details as JSON.

```bash
dail inspect myjail
```

### Execution

**`dail exec`** — Run a command inside a jail.

```bash
dail exec myjail ls /etc
dail exec myjail pkg install -y nginx
```

**`dail console`** — Open an interactive shell.

```bash
dail console myjail                 # default /bin/sh
dail console myjail --shell /bin/csh
```

### Logs

**`dail logs`** — View jail logs. By default reads CMD stdout/stderr (`cmd.log`). If `LOG` is set in Dailfile, reads that file from the jail rootfs instead. The log file is auto-created with write permissions at jail start.

```bash
dail logs myjail                        # CMD output (or LOG file if set in Dailfile)
dail logs myjail --tail 20              # last 20 lines
dail logs myjail -f                     # follow (like tail -f)
dail logs myjail --file /var/log/messages  # read arbitrary file from jail rootfs
```

Dailfile example:
```dockerfile
SERVICE postgresql
LOG /var/log/postgresql.log
```

### Build

**`dail build`** — Build a jail from a Dailfile.

```bash
dail build Dailfile --name myapp
dail build ./jails/web.dailfile --name web
dail build examples/postgres/Dailfile --name tmp --tag postgres:18  # build → save as image
```

### Cache

**`dail cache clean`** — Remove cached pkg packages and repository metadata.

```bash
dail cache clean
```

### Images

**`dail image save`** — Export a jail as a portable tar.zst archive.

```bash
dail image save myjail                      # tag: latest
dail image save myjail -t v1.0
dail image save myjail -t v1.0 -o /tmp/myjail.tar.zst
```

**`dail image load`** — Import an image from archive.

```bash
dail image load myjail-v1.0.tar.zst
```

**`dail image ls`** / **`dail images`** — List local images.

```bash
dail images
```

**`dail image inspect`** — Show image manifest details.

```bash
dail image inspect postgres:18
```

**`dail image rm`** — Remove a local image.

```bash
dail image rm myjail:v1.0
```

### Shell Completions

Dail supports dynamic completions — jail names, image refs and other values are completed at runtime.

```bash
# Dynamic completions (recommended — live jail name and image completion)
echo 'source <(COMPLETE=zsh dail)' >> ~/.zshrc
echo 'source <(COMPLETE=bash dail)' >> ~/.bashrc
COMPLETE=fish dail > ~/.config/fish/completions/dail.fish

# Static completions (subcommands and flags only, no live names)
dail completions zsh | doas tee /usr/local/share/zsh/site-functions/_dail > /dev/null
dail completions bash | doas tee /usr/local/etc/bash_completion.d/dail > /dev/null
dail completions fish > ~/.config/fish/completions/dail.fish
```

### Snapshots

**`dail snapshot`** — Create a ZFS snapshot (requires ZFS backend).

```bash
dail snapshot myjail                # tag: latest
dail snapshot myjail --tag v1.0
```

**`dail clone`** — Clone a jail from a snapshot.

```bash
dail clone myjail myjail-copy              # from latest
dail clone myjail:v1.0 myjail-copy         # from tagged snapshot
```

### Presets

**`dail preset`** — List available presets.

```bash
dail preset
```

## Presets

Presets apply common jail parameters in one flag:

| Preset | What it does |
|--------|-------------|
| `postgres` | `allow.sysvipc=true`, `persist=true` |
| `nginx` | `persist=true` |
| `dev` | `allow.raw_sockets=true`, `allow.sysvipc=true` |

Custom presets: create YAML (or TOML) files in `/var/db/dail/presets/`:

```yaml
# /var/db/dail/presets/myapp.yaml
description: "My custom app"
persist: true
params:
  allow.raw_sockets: "true"
limits:
  maxproc: "256"
```

## Configuration

Global config at `/usr/local/etc/dail/config.yaml` (TOML fallback supported):

```yaml
root_dir: /var/db/dail
storage_backend: directory  # or "zfs"
default_base: "15.0-RELEASE"
alias_interface: lo0
ip_pool: 10.100.0.0/24
mirror: https://download.freebsd.org/releases

# Optional: for ZFS backend
# zfs_pool: zroot
```

## Requirements

- FreeBSD 13+
- Rust (for building from source)
- ZFS (optional, for snapshot/clone features)

## Building

```bash
cargo build --release
```

## License

BSD-3-Clause
