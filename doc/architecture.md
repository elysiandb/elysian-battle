# Elysian-Battle — Architecture

> Internal architecture decisions and module design.

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────┐
│                   elysian-battle                     │
│                                                     │
│  ┌──────┐   ┌──────┐   ┌─────────┐   ┌──────────┐ │
│  │ CLI  │──▶│ Git  │──▶│ Builder │──▶│ Instance │ │
│  └──────┘   └──────┘   └─────────┘   └────┬─────┘ │
│      │                                      │       │
│      │         ┌────────────────────────────┘       │
│      │         ▼                                    │
│      │   ┌──────────┐                               │
│      │   │  Runner   │                               │
│      │   └────┬─────┘                               │
│      │        │                                     │
│      │   ┌────▼──────────────────────────────────┐  │
│      │   │           Test Suites                  │  │
│      │   │  ┌──────┐ ┌───────┐ ┌──────┐ ┌─────┐ │  │
│      │   │  │ CRUD │ │ Query │ │ Auth │ │ ... │ │  │
│      │   │  └──┬───┘ └──┬────┘ └──┬───┘ └──┬──┘ │  │
│      │   └─────┼────────┼────────┼────────┼─────┘  │
│      │         │        │        │        │         │
│      │   ┌─────▼────────▼────────▼────────▼─────┐  │
│      │   │        ElysianClient (HTTP)           │  │
│      │   │        TcpClient (TCP)                │  │
│      │   └──────────────────────────────────────┘  │
│      │                                             │
│      │   ┌──────────┐                              │
│      └──▶│  Report  │──▶ terminal + JSON file      │
│          └──────────┘                              │
└─────────────────────────────────────────────────────┘
```

---

## Execution Pipeline

The binary follows a strict sequential pipeline:

```
1. Parse CLI args
       │
2. Check prerequisites (git, go)
       │
3. Clone or update ElysianDB repo
       │
4. List branches/tags → prompt user (or use --version)
       │
5. Checkout requested version
       │
6. Build ElysianDB binary (go build)
       │
7. Find available ports
       │
8. Generate elysian.yaml config
       │
9. Start ElysianDB process
       │
10. Wait for health check
       │
11. Run test suites sequentially
       │  ├── setup()   → seed data
       │  ├── run()     → execute tests
       │  └── teardown()→ clean data
       │
12. Stop ElysianDB
       │
13. Generate report (terminal + file)
       │
14. Exit with appropriate code
```

---

## Module Dependency Graph

```
main.rs
  ├── cli.rs              (no internal deps)
  ├── prerequisites.rs    (no internal deps)
  ├── git.rs              (no internal deps)
  ├── builder.rs          (no internal deps)
  ├── config.rs           (depends on: port.rs)
  ├── port.rs             (no internal deps)
  ├── instance.rs         (depends on: config.rs)
  ├── client.rs           (no internal deps)
  ├── tcp_client.rs       (no internal deps)
  ├── runner.rs           (depends on: suites/*, client.rs, tcp_client.rs)
  ├── report.rs           (no internal deps)
  └── suites/
        ├── mod.rs         (trait definition)
        ├── health.rs      (depends on: client.rs)
        ├── crud.rs        (depends on: client.rs)
        ├── query.rs       (depends on: client.rs)
        ├── ...            (all depend on client.rs and/or tcp_client.rs)
        └── performance.rs (depends on: client.rs)
```

---

## Key Design Decisions

### 1. Sequential Suite Execution

Suites run one at a time, not in parallel. Reasons:
- **Isolation**: each suite can assume a clean or known database state.
- **Simplicity**: no need for per-suite port allocation or multiple instances.
- **Debugging**: failures are easier to reproduce and locate.

Within a suite, individual tests run sequentially too — many tests depend on state created by prior tests (e.g., create then read).

### 2. Single ElysianDB Instance

One ElysianDB process runs for the entire test session. Between suites, we reset data via the API (`POST /reset` or `DELETE /api/{entity}`). This is faster than restarting the process and closer to real usage patterns.

Exception: if a future suite needs a different configuration (e.g., `authentication.mode: "basic"`), we would stop, reconfigure, and restart. This is not needed in v0.1 since `user` mode covers all features.

### 3. Async Rust with Tokio

The project uses `tokio` as async runtime because:
- `reqwest` requires it for async HTTP.
- TCP client benefits from async I/O.
- Performance tests need concurrent request generation.
- Process management (spawn, wait, kill) integrates well.

### 4. `anyhow` for Infrastructure, Custom Errors for Tests

- Infrastructure code (git, build, instance) uses `anyhow::Result` — errors are fatal, context matters more than type.
- Test suites return `Vec<TestResult>` — never panic, never propagate errors. A test failure is a data point, not an error.

### 5. Cookie Jar for Session Auth

`reqwest::Client` is configured with a cookie jar enabled. After `POST /api/security/login`, the `edb_session` cookie is automatically stored and sent on subsequent requests. This mirrors real browser behavior.

### 6. Cleanup Between Suites

Each suite's `teardown()` method calls appropriate cleanup endpoints. The runner also has a `global_reset()` that performs **two distinct cleanup operations**:

1. **`POST /reset`** — clears all KV keys (this endpoint only affects the KV store, not entity data).
2. **`DELETE /api/{entity}`** — called for each known `battle_*` entity name to delete all documents.

Both are necessary because `/reset` does not touch entity data, and `DELETE /api/{entity}` does not touch KV keys. This ensures:
- Suite B never sees data from Suite A.
- Leftover data from a failed suite doesn't cascade.
- Both KV and entity stores are clean before each suite.

### 7. Report as First-Class Output

Reports are structured data (`Vec<SuiteResult>` containing `Vec<TestResult>`), serialized to both terminal (colored tables) and JSON files. The JSON report is the contract for CI integration — its schema is stable.

```rust
struct TestResult {
    suite: String,
    name: String,
    status: TestStatus,      // Passed | Failed | Skipped
    duration: Duration,
    error: Option<String>,   // failure message
    request: Option<String>, // HTTP method + path (for debugging)
    response_status: Option<u16>,
}

struct SuiteResult {
    name: String,
    tests: Vec<TestResult>,
    duration: Duration,
}

struct PerformanceResult {
    scenario: String,
    iterations: u64,
    p50: Duration,
    p95: Duration,
    p99: Duration,
    throughput: f64,  // requests/second
}

struct BattleReport {
    version: String,           // elysian-battle version
    elysiandb_version: String, // git ref tested
    timestamp: String,
    suites: Vec<SuiteResult>,
    performance: Vec<PerformanceResult>,
    total_passed: u64,
    total_failed: u64,
    total_skipped: u64,
    total_duration: Duration,
}
```

---

## Port Selection Strategy

```
1. Bind a TCP listener to 127.0.0.1:0
2. Read the OS-assigned port number
3. Close the listener immediately
4. Repeat for the second port (TCP)
5. Store both ports in Config
6. Generate elysian.yaml with these ports
7. Start ElysianDB — it binds to the same ports
```

There is a small TOCTOU window between closing our test listener and ElysianDB binding. This is acceptable because:
- We bind to `127.0.0.1` (no external interference).
- The OS typically doesn't reassign ports that quickly.
- If it fails, the health check will catch it and elysian-battle reports a startup error.

---

## Process Management

```rust
// Pseudocode for instance.rs

fn start(config_path: &Path, binary_path: &Path) -> Child {
    Command::new(binary_path)
        .arg("server")
        .arg("--config")
        .arg(config_path)
        .stdout(File::create(log_path))
        .stderr(File::create(log_path))
        .spawn()
}

fn wait_for_health(port: u16, timeout: Duration) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/health", port);
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() > deadline {
            return Err(anyhow!("Health check timeout"));
        }
        if reqwest::get(&url).await?.status() == 200 {
            return Ok(());
        }
        sleep(Duration::from_millis(500)).await;
    }
}

fn stop(child: &mut Child) {
    // Send SIGTERM
    signal::kill(child.id(), Signal::SIGTERM);
    // Wait up to 5s
    match timeout(Duration::from_secs(5), child.wait()).await {
        Ok(_) => {} // clean exit
        Err(_) => child.kill(), // force kill
    }
}
```

---

## Test Data Conventions

All test suites use predictable entity names to avoid collisions:

| Suite | Entity names |
|-------|-------------|
| CRUD | `battle_books`, `battle_authors` |
| Query | `battle_articles`, `battle_tags` |
| Schema | `battle_schema_test` |
| Auth | `battle_auth_data` |
| ACL | `battle_acl_data` |
| Transactions | `battle_tx_items` |
| KV | keys prefixed with `battle_kv_` |
| TCP | keys prefixed with `battle_tcp_` |
| Import/Export | `battle_export_test` |
| Hooks | `battle_hooked_entity` |
| Nested | `battle_posts`, `battle_comments`, `battle_users_nested` |
| Migrations | `battle_migrate_test` |
| Edge Cases | `battle_edge_*` |
| Crash Recovery | reuses `battle_crash_*` |
| Performance | `battle_perf_items` |

The `battle_` prefix ensures no collision with ElysianDB internal entities (`_elysiandb_core_*`).
