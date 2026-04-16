# CLAUDE.md — Project Conventions for Claude Code

## Project

Elysian-Battle: a Rust integration test harness for ElysianDB (a Go database).
It clones, builds, and tests any version of ElysianDB against 159 test scenarios.

## Build & Run

```bash
cargo build                  # debug build
cargo build --release        # release build
cargo test                   # run Rust unit/integration tests
cargo clippy                 # lint
cargo fmt -- --check         # format check
```

## Architecture

- Async Rust with **tokio** runtime (full features).
- HTTP client: **reqwest** with cookie jar enabled.
- CLI: **clap** v4 with derive macros.
- Error handling: **anyhow** for infrastructure code; test suites return `Vec<TestResult>` (never panic, never propagate errors).
- All test data uses the `battle_` prefix for entity names to avoid collisions with ElysianDB internals.

## Key Files

- `SPEC.md` — full technical specification
- `doc/test-scenarios.md` — authoritative test catalog (159 tests), source of truth for test counts
- `doc/architecture.md` — design decisions
- `doc/elysiandb-reference.md` — ElysianDB API reference

## Conventions

- Suites run sequentially, one ElysianDB instance for the whole session.
- Cleanup between suites: `POST /reset` (KV keys) + `DELETE /api/{entity}` per known `battle_*` entity.
- The crash recovery suite kills/restarts ElysianDB — it runs after all functional suites, before performance.
- Each test suite implements the `TestSuite` trait (setup/run/teardown).
- Performance tests are metrics-only (not pass/fail).

## Dependencies

All crates are listed in `Cargo.toml`. Key choices:
- `tabled` for terminal tables (not comfy-table).
- `dialoguer` + `console` + `indicatif` for interactive terminal UX.
- `async-trait` for the `TestSuite` trait.

## Git

All commits must be authored as:
- Name: `taymour`
- Email: `taymour.negib@gmail.com`

Use: `git -c user.name="taymour" -c user.email="taymour.negib@gmail.com" commit ...`

## Style

- Follow `cargo fmt` defaults.
- No `unwrap()` in production code — use `anyhow::Result` or collect errors into `TestResult`.
- `unwrap()` is acceptable in test setup/seed code where failure is fatal.
