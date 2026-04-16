# Elysian-Battle

**Comprehensive integration test harness and regression guard for [ElysianDB](https://github.com/elysiandb/elysiandb).**

Elysian-Battle is a standalone Rust binary that pulls any version of ElysianDB, compiles it from source, launches an isolated instance, runs 159 functional and performance tests, and produces a detailed report.

---

## Features

- Pull any ElysianDB version (branch, tag, or latest)
- Compile from source automatically (requires Go 1.24+)
- Fully isolated: auto-selected ports, dedicated data directory, internal storage engine
- 159 test scenarios covering every documented API endpoint
- Performance benchmarks with latency percentiles
- Interactive mode (prompts) and CI mode (CLI flags)
- JSON and terminal reports

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | stable 1.75+ | [rustup.rs](https://rustup.rs) |
| Go | 1.24+ | [go.dev/dl](https://go.dev/dl/) |
| Git | any recent | system package manager |

---

## Quick Start

```bash
# Build
cargo build --release

# Run (interactive mode)
./target/release/elysian-battle

# Run (CI mode — test the main branch)
./target/release/elysian-battle --version main --report json
```

---

## CLI Options

```
elysian-battle [OPTIONS]

Options:
  --version <REF>     Branch name, tag, or "latest" (skips interactive prompt)
  --suite <NAMES>     Comma-separated suite names to run (default: all)
  --report <FORMAT>   Output format: text (default) or json
  --no-build          Skip compilation, reuse existing binary
  --keep-alive        Don't stop ElysianDB after tests
  --verbose           Show detailed logs
  --help              Show help
```

---

## Test Suites

| Suite | Tests | Covers |
|-------|-------|--------|
| `health` | 5 | Health check, stats, config, save, version header |
| `crud` | 24 | Create, read, update, delete, batch, count, exists |
| `query` | 20 | POST /api/query with all filter operators |
| `query_params` | 8 | URL-based filtering, sorting, pagination |
| `nested` | 6 | @entity creation, includes expansion |
| `schema` | 10 | Auto-inference, manual schema, strict mode |
| `auth` | 15 | Token, session, user management |
| `acl` | 10 | Permissions, ownership, admin bypass |
| `transactions` | 8 | Begin, write, commit, rollback, isolation |
| `kv` | 8 | KV HTTP API with TTL |
| `tcp` | 8 | TCP protocol commands |
| `import_export` | 4 | Full database dump and restore |
| `hooks` | 7 | JavaScript hooks (pre_read, post_read) |
| `migrations` | 3 | Bulk data updates |
| `edge_cases` | 12 | Unicode, large payloads, concurrency |
| `crash_recovery` | 3 | SIGKILL, WAL replay, shard corruption |
| `performance` | 8 | Latency percentiles, throughput |

---

## Reports

Reports are saved to `.battle/reports/` as JSON files. Example:

```
.battle/reports/
├── latest.json              # symlink to most recent
└── 2026-04-16T14-30-00.json
```

Exit codes:
- `0` — all tests passed
- `1` — one or more tests failed
- `2` — infrastructure error (build, startup, prerequisites)

---

## Project Structure

```
elysian-battle/
├── Cargo.toml
├── README.md
├── SPEC.md                    # Full technical specification
├── doc/
│   ├── architecture.md        # Architecture decisions
│   ├── test-scenarios.md      # Complete test catalog (156 tests)
│   └── elysiandb-reference.md # ElysianDB API reference
├── CLAUDE.md                  # Claude Code project conventions
└── src/
    ├── main.rs                # Entry point & orchestration
    ├── cli.rs                 # CLI parsing & prompts
    ├── prerequisites.rs       # Go/Git checks
    ├── git.rs                 # Repository management
    ├── builder.rs             # Go build orchestration
    ├── config.rs              # Config generation
    ├── instance.rs            # ElysianDB process lifecycle
    ├── port.rs                # Port selection
    ├── client.rs              # HTTP API client
    ├── tcp_client.rs          # TCP protocol client
    ├── runner.rs              # Test orchestrator
    ├── report.rs              # Report generation
    └── suites/                # One file per test suite
        ├── mod.rs
        ├── health.rs
        ├── crud.rs
        ├── query.rs
        └── ...
```

---

## CI Integration

```yaml
# GitHub Actions example
- name: Run Elysian-Battle
  run: |
    cargo build --release
    ./target/release/elysian-battle \
      --version ${{ github.head_ref || 'main' }} \
      --report json \
      --verbose
```

---

## Documentation

- [Technical Specification](SPEC.md) — full project spec
- [Architecture](doc/architecture.md) — design decisions
- [Test Scenarios](doc/test-scenarios.md) — complete test catalog
- [ElysianDB Reference](doc/elysiandb-reference.md) — API reference

---

## License

MIT
