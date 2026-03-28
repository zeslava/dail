# Dail
Daily jail management for FreeBSD

## Features

- **Familiar workflow:** `create`, `start`, `stop`, `rm`, `run`, `exec`, `build`
- **`.dail` files:** declarative jail builds
- **Presets:** one flag to configure common workloads (`--preset postgres`)
- **Thick & thin jails:** full copy or shared base with per-jail overlay
- **Networking:** inherit, IP alias, or VNET with bridge
- **Port forwarding:** `-p host_port:jail_port` via PF rdr anchors
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

## `.dail` File Reference

A `.dail` file describes how to build and run a jail — similar to a Dockerfile.

```dockerfile
# Base FreeBSD release
FROM 15.0-RELEASE

# Install packages
RUN pkg install -y nginx

# Copy config files from build context into the jail
COPY nginx.conf /usr/local/etc/nginx/nginx.conf

# Set environment variables (appended to /etc/profile)
ENV APP_ENV=production

# Jail parameters (FreeBSD jail.conf options)
PARAM persist=true
PARAM allow.raw_sockets=true

# Mount host directories into the jail
MOUNT /home/user/site:/usr/local/www
MOUNT ro:/data:/mnt/data

# Enable a service (creates user/group/dirs, adds to rc.conf)
SERVICE nginx --no-user

# Log file path (used by `dail logs`)
LOG /var/log/nginx/access.log

# Port forwarding (host_port:jail_port)
EXPOSE 8080:80
EXPOSE 443

# Override default startup command
CMD /usr/local/sbin/nginx -g "daemon off;"
```

| Directive | Syntax | Description |
|-----------|--------|-------------|
| `FROM` | `FROM <release>` | FreeBSD base release |
| `RUN` | `RUN <command>` | Execute command during build |
| `COPY` | `COPY <src> <dst>` | Copy files from build context into jail |
| `ENV` | `ENV <KEY>=<VALUE>` | Set environment variable |
| `PARAM` | `PARAM <key>=<value>` | Set jail parameter |
| `MOUNT` | `MOUNT [ro:]<src>:<dst>` | Mount host directory (optional `ro:` prefix) |
| `SERVICE` | `SERVICE <name> [--no-user]` | Enable service, create user/group/dirs |
| `LOG` | `LOG <path>` | Log file for `dail logs` |
| `EXPOSE` | `EXPOSE [host:]<port>` | Port forwarding (overridden by `-p`) |
| `CMD` | `CMD <command>` | Startup command (overrides SERVICE default) |

### Example: deploy your app

```bash
# 1. Create a .dail file in your project
cat > myapp.dail <<'EOF'
FROM 15.0-RELEASE
RUN pkg install -y go
COPY . /opt/myapp
RUN cd /opt/myapp && go build -o /usr/local/bin/myapp
EXPOSE 8080
CMD /usr/local/bin/myapp
EOF

# 2. Build and run
dail run myapp --build myapp.dail

# 3. Check logs and status
dail logs myapp
dail ls
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
dail create web -p 8080:80                                  # port forwarding via PF
dail create app --uid 1001                                   # service user with specific UID/GID
```

**`dail run`** — Create and start a jail in one step. Same options as `create`, plus `--rm`, `--build`, `--rebuild`.

```bash
dail run myjail                                             # create + start
dail run postgres-jail --preset postgres                    # with preset
dail run temp --rm                                          # auto-remove on stop
dail run web --vnet --vnet-ip 10.0.0.5/24 --vnet-gateway 10.0.0.1
dail run app --mount /data:/app --preset dev --limit maxproc=512
dail run postgres.dail                                      # build + start, name from filename
dail run postgres.dail --name pg                            # build with explicit name
dail run postgres.dail --rebuild                            # rebuild from scratch
dail run https://github.com/user/repo.git --name app        # build from git repo
dail run https://github.com/user/repo//jails/web --name web # build from subdirectory
dail run web -p 8080:80                                     # port forwarding
dail run web -p 8080:80/tcp -p 5432:5432                    # multiple ports
dail run app.dail --uid 1001                                 # service user with specific UID/GID
```

**`dail start`** / **`stop`** / **`restart`** — Manage jail state.

```bash
dail start myjail
dail stop myjail                    # if --rm was set, jail is auto-removed
dail stop --all                     # stop all running jails
dail restart myjail
dail restart --all                  # restart all running jails
```

**`dail rm`** — Remove a jail and its filesystem.

```bash
dail rm myjail                      # must be stopped
dail rm myjail --force              # stop + remove
dail rm --all --force               # remove all jails
```

### Inspection

**`dail ls`** — List jails.

```bash
dail ls                             # all jails (colored status)
dail ls --running                   # only running
dail ls --format json               # JSON output
dail ls -q                          # names only (for scripting)
```

**`dail inspect`** — Show jail details.

```bash
dail inspect myjail                 # human-readable
dail inspect myjail --json          # raw JSON
```

**`dail config show`** — Display current configuration.

```bash
dail config show
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

**`dail logs`** — View jail logs. By default reads CMD stdout/stderr (`cmd.log`). If `LOG` is set in the `.dail` file, reads that file from the jail rootfs instead. The log file is auto-created with write permissions at jail start.

```bash
dail logs myjail                        # CMD output (or LOG file if set in .dail)
dail logs myjail --tail 20              # last 20 lines
dail logs myjail -f                     # follow (like tail -f)
dail logs myjail --file /var/log/messages  # read arbitrary file from jail rootfs
```

`.dail` file example:
```dockerfile
SERVICE postgresql          # auto-creates user/group/dirs, use --no-user to skip
LOG /var/log/postgresql.log
EXPOSE 5432                 # port forwarding (host_port:jail_port or just port)
```

### Monitoring

**`dail top`** — Show running processes inside a jail.

```bash
dail top myjail                     # watch mode (refreshes every 2s)
dail top myjail --once              # single snapshot
```

### Build

**`dail build`** — Build a jail from a `.dail` file or git URL.

```bash
dail build pg.dail --name myapp
dail build ./jails/web.dail --name web
dail build https://github.com/user/repo.git --name app                  # build from git repo
dail build https://github.com/user/repo//jails/web --name web           # build from subdirectory
```

### Cache

**`dail cache clean`** — Remove cached pkg packages and repository metadata.

```bash
dail cache clean
```

### Shell Completions

Dail supports dynamic completions — jail names and other values are completed at runtime.

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

## Network Modes

Dail supports multiple networking configurations:

### Auto IP Allocation (default)

When no network flags are specified, dail automatically allocates an IP from the configured pool:

```bash
dail run myjail                      # Auto-allocates 10.100.0.X
# Output: IP allocated: 10.100.0.2 on lo0
```

Pool configured in `/usr/local/etc/dail/config.yaml`:

```yaml
ip_pool: 10.100.0.0/24
alias_interface: lo0
```

### Explicit IP Alias

Assign a specific IP address:

```bash
dail create web --ip 10.100.0.50/24
```

**Note:** Dail validates that the IP is not already in use by another jail.

### VNET (Virtual Network)

Full network stack isolation with bridged networking:

```bash
dail create app --vnet --vnet-ip 10.0.0.5/24 --vnet-gateway 10.0.0.1 --vnet-bridge bridge0
```

### Port Forwarding

Forward host ports to jail ports using PF rdr anchors:

```bash
dail run web -p 8080:80                    # forward host:8080 → jail:80
dail run web -p 8080:80/tcp -p 5432:5432   # multiple ports, optional proto
```

Requires PF enabled with the dail anchor in `/etc/pf.conf`:

```
rdr-anchor "dail/*"
```

In `.dail` files, use `EXPOSE` to declare default port mappings:

```dockerfile
EXPOSE 5432
EXPOSE 8080:80
```

If `-p` is passed on the CLI, all `EXPOSE` directives are ignored.

### Host Network (inherit)

Share the host's network stack:

```bash
dail create legacy --network inherit
```

### No Network (isolated)

Completely isolated jail with no network access:

```bash
dail create isolated --network none
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
