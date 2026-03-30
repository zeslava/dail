# Changelog

## [0.1.2] - 2026-03-30

### Added
- Publish pre-built `dail-freebsd-amd64` binary to GitHub Releases on every `v*` tag push
- Separate `build.yml` workflow for CI checks on pushes to `main`

## [0.1.1] - 2026-03-30

### Added
- FreeBSD release build workflow using vmactions ([#1](https://github.com/zeslava/dail/pull/1))

### Fixed
- Install `curl` via `pkg` in the FreeBSD VM before running the Rust installer ([#2](https://github.com/zeslava/dail/pull/2))

## [0.1.0] - Initial release

- Initial project structure and CLI commands
