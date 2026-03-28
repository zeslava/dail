# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read AGENTS.md for project instructions, architecture, and conventions.

## Build & Dev Commands

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo clippy                   # lint
cargo test                     # tests
cargo run -- <args>            # run with arguments
task build                     # release build
task run                       # build and run (pass args via CLI_ARGS)
task install                   # build release + install to /usr/local/bin
```
