# TODO

## Done

- [x] **Security:** command injection, shell injection, path traversal, root check
- [x] **Fix shell injection in `directory.rs`** — replaced `sh -c "cp -a base/* root/"` with direct `cp -a base/. root/`
- [x] **Data integrity:** flock, corrupt state backup, atomic writes, image save registration
- [x] **Validation:** `--type`, `--vnet`/`--vnet-ip`, arch detection, build/run cleanup, unwrap audit

## Phase 4: Critical UX fixes

- [x] **State reconciliation** — `jls` cross-reference on DailStore load, auto-fixes stale "running" after reboot
- [x] **"Not initialized" detection** — clear error if `root_dir` missing; `config init` prints next step
- [x] **Bootstrap progress** — fetch stderr inherited (shows download progress), extraction message added
- [x] **Clone IP conflict** — auto-allocates new IP from pool instead of copying source IP
- [x] **Actionable error messages** — all `InvalidState` errors now suggest the fix command

## Phase 5: Convenience UX

- [x] **`--all` flag** for `stop`, `rm`, `restart`
- [x] **Color output** — running=green, stopped=red, created=yellow in `dail ls`
- [x] **`config show`** — display current config file
- [x] **`--quiet` / `-q`** flag for `ls` and `images` — output names only for scripting
- [x] **`inspect` human-readable mode** — structured table by default, `--json` for raw JSON
- [ ] **`dail update <jail>`** — modify mounts/params/limits/preset without recreating
- [x] **Unified table formatting** — one implementation for ls, images, preset tables; auto-width
- [x] **Completions** — add `--preset` names, `--type` (thick/thin), `--network` (inherit/none), `--base` releases

## Phase 6: Code quality

- [x] **Extract shared builder** — deduplicate ~100 lines between `create.rs` and `run.rs`
- [ ] **Remove dead code** — audit `#[allow(dead_code)]` in `src/freebsd/mod.rs`
- [ ] **Add unit tests** — .dail parser, `next_free_ip()`, `validate_jail_name()`, store serde round-trip
- [ ] **Extract IP allocation** — deduplicate between `shared.rs` and `clone.rs` into a reusable helper
- [x] **Protect build against crash-state** — build uses in-memory overrides via `start_for_build`, persisted state never mutated
- [ ] **Split `run.rs`** — extract build-mode into separate function/module
- [x] **Git URL support in `build` and `run`** — clone git repo, find .dail file, build+run; supports `//subdir` for nested projects
- [x] **`dail top`** — show running processes inside a jail (jexec ps wrapper)
- [x] **Image ref parsing** — extract helper, used in 4+ places
- [ ] **Deprecation** — migrate `serde_yaml` → `serde_yml` or drop YAML support (TOML only)

## Phase 7: Tests

- [ ] .dail parser — `Dailfile::parse()` (pure logic)
- [ ] IP allocation — `next_free_ip()` (pure logic)
- [ ] Jail name validation — `validate_jail_name()` (pure logic)
- [ ] Store round-trip — `DailStore` serialize/deserialize with tempdir
- [ ] Preset loading — `Preset::load` / `builtin()`
- [ ] Config loading — `GlobalConfig` YAML/TOML fallback

## Phase 8: Packaging & docs

- [ ] Man page (`dail.1`)
- [x] Document `--mount-ro` and `--network` flags in README
- [x] Document COPY host-path behavior in .dail file reference
- [ ] Submit port to FreeBSD ports tree

---

## Future

### Other ideas

- [ ] `dail cp <jail>:<path> <local>` — copy files from/to jail
- [ ] Health checks in .dail files (`HEALTHCHECK`)
- [ ] Volumes — named persistent storage
- [ ] `dail kill` — force-stop without removal
- [ ] `dail stats` — resource usage via rctl
- [ ] logs: kqueue instead of busy-wait polling
