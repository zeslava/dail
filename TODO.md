# TODO

## Done

- [x] Simplify postgres (and other services) network exposure — solved via COPY with relative paths
  from Dailfile context directory. Example configs in `examples/postgres/`.
- [x] YAML as primary config/preset format (TOML fallback supported)

## Images — local image system

Step-by-step plan to support reusable images (build once, run many).

### Phase 1: Export / Import ✅

- [x] `dail image save <jail> [-t tag] [-o file]` — pack jail rootfs + config into tar.zst archive
- [x] `dail image load <file.tar.zst>` — unpack archive into local image store (`/var/db/dail/images/<name>/<tag>/`)
- [x] `dail image ls` / `dail images` — list local images
- [x] `dail image rm <name>:<tag>` — remove local image

### Phase 2: Run from image ✅

- [x] `dail run <name> --image <image>:<tag>` — create jail from local image instead of bootstrap
  - Copy rootfs from image store (or ZFS clone if ZFS backend)
  - Apply manifest config as defaults, CLI args override
- [x] `dail build` with `--tag` — build Dailfile and save result as image
  - `dail build examples/postgres/Dailfile --tag postgres:18`
  - Equivalent to build temp jail → image save → remove temp jail

### Phase 3: Image management

- [x] Thin jails from images — shared readonly rootfs + writable skeleton per jail
  - `image.ensure_rootfs()` extracts once to `images/<name>/<tag>/rootfs/`
  - `start_inner()` resolves base from `image_ref` or `base`
  - `--type thick` still available for full copy
- [x] Track image dependents — don't delete shared rootfs while jails use it
- [x] `dail image inspect <image>:<tag>` — show manifest details
- [ ] Image deduplication — shared base layers via ZFS clones

### Phase 4: Remote registry (future)

- [ ] `dail push <image>:<tag> <registry-url>` — upload image to HTTP registry
- [ ] `dail pull <registry-url>/<image>:<tag>` — download and store locally
- [ ] Simple registry protocol: PUT/GET tar.zst + manifest over HTTPS
- [ ] `Dailfile` FROM support: `FROM postgres:18` (local image) vs `FROM 15.0-RELEASE` (base)

### Phase 5: Daemon mode (run without sudo/doas)

- [ ] `daild` — root daemon listening on `/var/run/dail.sock` (unix socket)
- [ ] `dail` CLI becomes a client — sends commands over socket, prints responses
- [ ] Socket permissions via `dail` group (`srw-rw---- root:dail`)
- [ ] Protocol: JSON-over-unix-socket (or HTTP over unix socket like Docker)
- [ ] `rc.d/daild` — FreeBSD service script for daemon lifecycle
- [ ] Streaming support for `exec`, `console`, `build` output

## Other ideas

- [x] `dail logs <jail>` — tail jail console output
- [ ] `dail cp <jail>:<path> <local>` — copy files from/to running jail
- [ ] Health checks in Dailfile (`HEALTHCHECK` instruction)
- [ ] Volumes — named persistent storage decoupled from jail lifecycle
