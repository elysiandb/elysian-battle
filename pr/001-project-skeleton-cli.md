# PR — #1 Project skeleton & CLI

**Branch:** `feat/1-project-skeleton-cli`
**Base:** `main`
**Issue:** #1

## Title

Add project skeleton: CLI, prerequisites, and port selection

## Summary

- CLI argument parsing with clap derive: `--version`, `--suite`, `--report`, `--no-build`, `--keep-alive`, `--verbose`
- Interactive mode with dialoguer prompts when `--version` is omitted (select branch/tag/latest)
- Prerequisites checker: verifies `git` is installed and `go` >= 1.24, with clear error messages and install links
- Port selection: finds two distinct ephemeral TCP ports via OS assignment on `127.0.0.1:0`
- Main entry point with tokio async runtime, banner display, and pipeline stubs (steps 4-12) for future tickets
- 6 unit tests covering Go version parsing/validation and port selection

## Files changed

- `src/main.rs` — Entry point, orchestration pipeline
- `src/cli.rs` — CLI parsing + interactive prompts
- `src/prerequisites.rs` — Git/Go version checks
- `src/port.rs` — Ephemeral port selection
- `Cargo.lock` — Dependency lock file

## Test plan

- [ ] `cargo build` compiles without errors
- [ ] `cargo clippy` passes with no warnings
- [ ] `cargo fmt -- --check` reports no diffs
- [ ] `cargo test` — 6 tests pass (Go version parsing, version validation, port selection)
- [ ] `./target/debug/elysian-battle --help` shows all 6 options
- [ ] `./target/debug/elysian-battle --version latest --verbose` prints banner, prerequisites, ports
- [ ] `./target/debug/elysian-battle` (no args) triggers interactive version prompt
