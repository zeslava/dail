# TODO

## Images

### Phase 3: Image management (remaining)

- [ ] Image deduplication — shared base layers via ZFS clones

### Phase 4: Remote registry

- [ ] `dail push <image>:<tag> <registry-url>` — upload image to HTTP registry
- [ ] `dail pull <registry-url>/<image>:<tag>` — download and store locally
- [ ] Simple registry protocol: PUT/GET tar.zst + manifest over HTTPS
- [ ] `Dailfile` FROM support: `FROM postgres:18` (local image) vs `FROM 15.0-RELEASE` (base)

### Phase 5: Daemon mode

- [ ] `daild` — root daemon listening on `/var/run/dail.sock` (unix socket)
- [ ] `dail` CLI becomes a client — sends commands over socket, prints responses
- [ ] Socket permissions via `dail` group (`srw-rw---- root:dail`)
- [ ] `rc.d/daild` — FreeBSD service script for daemon lifecycle
- [ ] Streaming support for `exec`, `console`, `build` output

## Other ideas

- [ ] `dail cp <jail>:<path> <local>` — copy files from/to running jail
- [ ] Health checks in Dailfile (`HEALTHCHECK` instruction)
- [ ] Volumes — named persistent storage decoupled from jail lifecycle

## Packaging

- [x] FreeBSD port skeleton (`port/`)
- [ ] Submit port to FreeBSD ports tree
- [ ] Man page (`dail.1`)
