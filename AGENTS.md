# Project: Dail

## What This Project Does
FreeBSD jail manager written in Rust. Docker-like UX (Dailfile, create/start/stop/rm) with jail-native semantics. Inspired by BastilleBSD and Docker.

## Stack
- **Language:** Rust (edition 2024)
- **Key libs:** clap 4, serde (json/yaml/toml), thiserror 2, anyhow, tracing, chrono, uuid

## Project Structure
```
src/
  main.rs              # CLI entrypoint (clap)
  error.rs             # DailError enum
  store.rs             # jail metadata persistence (JSON)
  output.rs            # output formatting
  commands/            # CLI command handlers (one file per command)
    create.rs, run.rs, start.rs, stop.rs, rm.rs, ls.rs,
    exec.rs, console.rs, inspect.rs, restart.rs,
    build.rs, bootstrap.rs, snapshot.rs, clone.rs,
    preset.rs, config_init.rs, config_show.rs,
    top.rs, cache.rs, image.rs, logs.rs, shared.rs
  jail/                # jail core logic
    config.rs          # GlobalConfig, JailConfig, JailType, MountSpec
    state.rs           # JailState, JailStatus
    lifecycle.rs       # JailLifecycle (create/start/stop/remove/exec)
    preset.rs          # Preset system (builtin + user TOML presets)
  freebsd/             # low-level FreeBSD bindings
    jail_sys.rs        # jail create/remove/exec via libc
    mount.rs           # nullfs/devfs mount
    ifconfig.rs        # interface management
    zfs.rs             # ZFS operations
    rctl.rs            # resource limits (rctl)
  storage/             # jail filesystem backends
    mod.rs, directory.rs, zfs_backend.rs
  network/             # networking modes (alias, vnet, inherit)
    mod.rs
  build/               # Dailfile parser + executor
    mod.rs, dailfile.rs, executor.rs
```

## Key Rules for This Project
- Single binary crate, no workspace
- FreeBSD-only (uses jail(2), nullfs, devfs, rctl)
- `thiserror` for DailError, `anyhow` in command handlers
- No `unwrap()` without safety comment
- Presets: builtin hardcoded + user TOML in `/var/db/dail/presets/`
- GlobalConfig lives at `/usr/local/etc/dail/config.toml`

## What NOT to Read (save tokens)
- target/
- .git/

## Entry Points (start here if exploring)
- `src/main.rs` — CLI commands enum, dispatch
- `src/jail/config.rs` — GlobalConfig + JailConfig
- `src/jail/lifecycle.rs` — core jail operations
- `src/commands/` — individual command implementations

## Common Commands
```bash
cargo build           # build
cargo clippy          # lint
cargo test            # tests (when added)
```
