# Elysian-Battle — Technical & Functional Specification

> **Version:** 0.1.0-draft
> **Date:** 2026-04-16
> **Status:** Draft — awaiting review before implementation

---

## 1. Project Purpose

**Elysian-Battle** is a standalone Rust binary that acts as a comprehensive integration test harness and regression guard for [ElysianDB](https://github.com/elysiandb/elysiandb).

### Goals

1. **Pull** any version of ElysianDB (branch, tag, or `latest`) from the public Git repository.
2. **Compile** the Go binary from source.
3. **Launch** a fully isolated ElysianDB instance (dedicated ports, dedicated data directory, `internal` storage engine — zero external dependencies).
4. **Execute** a broad suite of functional, edge-case, and performance tests covering every documented API surface.
5. **Report** a clear, human-readable summary of pass/fail results and performance metrics.

### Non-goals (for v0.1)

- MongoDB engine testing (deferred to a later phase).
- Admin UI browser testing.
- ElysianGate (clustering) testing.
- JavaScript hook execution correctness (tested indirectly via API).

---

## 2. Target Users

| User | Usage |
|------|-------|
| ElysianDB maintainers | Run before merging a PR to catch regressions. |
| CI pipelines | Automated quality gate on every push / PR. |
| Contributors | Validate their changes locally in one command. |

---

## 3. Runtime Prerequisites

| Dependency | Why | Version |
|------------|-----|---------|
| **Rust toolchain** | Build elysian-battle itself | stable (1.75+) |
| **Go toolchain** | Compile ElysianDB from source | 1.24+ |
| **Git** | Clone / checkout ElysianDB | any recent |
| **Network** | Initial clone only (tests run offline) | — |

The binary must check for Go and Git at startup and fail with a clear error if missing.

---

## 4. User Interaction Flow

```
$ ./elysian-battle

  ╔══════════════════════════════════════╗
  ║       ELYSIAN-BATTLE  v0.1.0        ║
  ╚══════════════════════════════════════╝

  Checking prerequisites...
    ✓ git 2.43.0
    ✓ go 1.24.7

  Fetching ElysianDB repository...
    ✓ Repository ready at .battle/elysiandb

  Available versions:
    Branches: main, feat/add-tests, feat/engine-mongodb, ...
    Tags:     v0.1.14, v0.1.13, v0.1.12, ...

  ? Select version source:
    > branch
      tag
      latest (main HEAD)

  ? Enter branch name: main

  Building ElysianDB (main)...
    ✓ Build succeeded (3.2s)

  Configuring isolated instance...
    HTTP port: 19201 (auto-selected)
    TCP  port: 19202 (auto-selected)
    Data dir:  .battle/data/
    Engine:    internal

  Starting ElysianDB...
    ✓ Health check passed

  Running test suites...
    [████████████████████░░░░] 131/159

  ══════════════ REPORT ══════════════

  Suite: Health & System     5/5   ✓
  Suite: Entity CRUD        24/24  ✓
  Suite: Query API           20/20  ✓
  Suite: Query Params        8/8   ✓
  Suite: Nested Entities     6/6   ✓
  Suite: Schema              10/10 ✓
  Suite: Authentication      15/15 ✓
  Suite: ACL                 0/10  ✗ (10 failures)
  Suite: Transactions        8/8   ✓
  Suite: KV Store            8/8   ✓
  Suite: TCP Protocol        8/8   ✓
  Suite: Import/Export       4/4   ✓
  Suite: Hooks               7/7   ✓
  Suite: Migrations          3/3   ✓
  Suite: Edge Cases          12/12 ✓
  Suite: Crash Recovery      3/3   ✓
  Suite: Performance         —     (see below)

  Performance Summary:
  ┌──────────────────────┬────────┬────────┬────────┐
  │ Scenario             │ p50    │ p95    │ p99    │
  ├──────────────────────┼────────┼────────┼────────┤
  │ Single create        │ 1.2ms  │ 2.1ms  │ 3.4ms  │
  │ Batch create (100)   │ 8.3ms  │ 12ms   │ 15ms   │
  │ List (1000 docs)     │ 4.1ms  │ 6.8ms  │ 9.2ms  │
  │ Filtered query       │ 2.3ms  │ 4.5ms  │ 7.1ms  │
  └──────────────────────┴────────┴────────┴────────┘

  Total: 141/151 passed, 10 failed, 0 skipped
  Duration: 42.3s

  Full report: .battle/reports/2026-04-16T14-30-00.json
```

### CLI Arguments (non-interactive mode for CI)

```
elysian-battle --version main          # branch name
elysian-battle --version v0.1.14       # tag
elysian-battle --version latest        # HEAD of main
elysian-battle --suite crud,query      # run specific suites only
elysian-battle --report json           # output format: json | text (default)
elysian-battle --no-build              # skip build (reuse last binary)
elysian-battle --keep-alive            # don't stop ElysianDB after tests
elysian-battle --verbose               # detailed logs
```

---

## 5. Directory Layout (runtime)

All runtime artifacts live under `.battle/` in the current working directory:

```
.battle/
├── elysiandb/              # Cloned ElysianDB repository
├── bin/
│   └── elysiandb           # Compiled Go binary
├── data/                   # ElysianDB data directory (store.folder)
├── config/
│   └── elysian.yaml        # Generated configuration file
├── logs/
│   └── elysiandb.log       # Server stdout/stderr capture
└── reports/
    ├── latest.json          # Symlink to most recent report
    └── 2026-04-16T14-30-00.json
```

---

## 6. Project Source Layout (Rust)

```
elysian-battle/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── SPEC.md                          # This file
├── doc/
│   ├── architecture.md              # Architecture decisions
│   ├── test-scenarios.md            # Complete test scenario catalog
│   └── elysiandb-reference.md       # ElysianDB API reference
├── src/
│   ├── main.rs                      # Entry point, CLI parsing, orchestration
│   ├── cli.rs                       # CLI argument parsing & interactive prompts
│   ├── prerequisites.rs             # Go/Git version checks
│   ├── git.rs                       # Git clone, fetch, checkout, branch/tag listing
│   ├── builder.rs                   # Go build orchestration
│   ├── config.rs                    # ElysianDB YAML config generation
│   ├── instance.rs                  # ElysianDB process lifecycle (start, health, stop)
│   ├── port.rs                      # Available port detection
│   ├── client.rs                    # HTTP client for ElysianDB REST API
│   ├── tcp_client.rs                # TCP client for ElysianDB TCP protocol
│   ├── report.rs                    # Test result aggregation & output formatting
│   ├── runner.rs                    # Test suite orchestrator
│   └── suites/
│       ├── mod.rs                   # Suite registration & trait definition
│       ├── health.rs                # Health & system endpoints
│       ├── crud.rs                  # Entity CRUD operations
│       ├── query.rs                 # Query API (POST /api/query)
│       ├── query_params.rs          # URL-based filtering, sorting, pagination
│       ├── schema.rs                # Schema inference & validation
│       ├── auth.rs                  # Authentication (token, basic, user modes)
│       ├── acl.rs                   # Access control lists
│       ├── transactions.rs          # Transaction API
│       ├── kv.rs                    # KV HTTP API
│       ├── tcp.rs                   # TCP protocol commands
│       ├── import_export.rs         # Import/Export API
│       ├── hooks.rs                 # JavaScript hooks
│       ├── migrations.rs            # Migration API
│       ├── nested.rs                # Nested entity creation (@entity)
│       ├── edge_cases.rs            # Edge cases & error handling
│       ├── crash_recovery.rs        # Crash recovery & WAL replay
│       └── performance.rs           # Latency & throughput benchmarks
└── tests/
    └── integration.rs               # Smoke test for elysian-battle itself
```

---

## 7. Core Modules — Responsibilities

### 7.1 `cli.rs` — Command-line Interface

- Parse CLI arguments with `clap`.
- Interactive mode: prompt for version source (branch/tag/latest), display available options.
- Non-interactive mode: accept `--version`, `--suite`, `--report`, `--no-build`, `--keep-alive`, `--verbose` flags.

### 7.2 `prerequisites.rs` — Environment Checks

- Run `git --version` and `go version`, parse output.
- Verify Go >= 1.24 (required by ElysianDB's `go.mod`).
- Return structured errors with install instructions if missing.

### 7.3 `git.rs` — Repository Management

- Clone `https://github.com/elysiandb/elysiandb.git` into `.battle/elysiandb/` if not present.
- Fetch latest refs on each run.
- List remote branches and tags.
- Checkout requested ref (branch, tag, or commit SHA).
- Detect current checked-out ref for reporting.

### 7.4 `builder.rs` — Go Compilation

- Run `go build -trimpath -ldflags="-s -w" -o ../.battle/bin/elysiandb .` from the cloned repo.
- Set `CGO_ENABLED=0` for pure Go build.
- Capture and surface build errors clearly.
- Skip build if `--no-build` flag and binary already exists.
- Report build duration.

### 7.5 `config.rs` — Configuration Generator

- Generate `elysian.yaml` with:
  - `engine.name: "internal"`
  - `server.http.port`: auto-selected available port
  - `server.tcp.port`: auto-selected available port
  - `store.folder`: `.battle/data/`
  - `security.authentication.enabled: true`
  - `security.authentication.mode: "user"` (most features to test)
  - `api.schema.enabled: true`
  - `api.schema.strict: false` (will be toggled per test)
  - `api.hooks.enabled: true`
  - `api.cache.enabled: true`
  - `stats.enabled: true`
  - `adminui.enabled: false` (no browser testing)
- Serialize to YAML using `serde` + `serde_yaml`.

### 7.6 `instance.rs` — Process Lifecycle

- Start ElysianDB binary with generated config path (`--config`).
- Redirect stdout/stderr to log file.
- Poll `GET /health` until 200 (timeout: 30s, interval: 500ms).
- Graceful shutdown via SIGTERM, then SIGKILL after 5s.
- Clean data directory between test suites if needed.

### 7.7 `port.rs` — Port Selection

- Bind TCP socket to port 0, read assigned port, close socket.
- Select two consecutive available ports for HTTP and TCP.
- Verify ports are still free before starting ElysianDB.

### 7.8 `client.rs` — HTTP Client

Thin wrapper around `reqwest` providing typed methods for every ElysianDB endpoint:

```rust
impl ElysianClient {
    // Entity CRUD
    async fn create(&self, entity: &str, body: Value) -> Result<Response>;
    async fn list(&self, entity: &str, params: ListParams) -> Result<Response>;
    async fn get(&self, entity: &str, id: &str) -> Result<Response>;
    async fn update(&self, entity: &str, id: &str, body: Value) -> Result<Response>;
    async fn batch_update(&self, entity: &str, body: Value) -> Result<Response>;
    async fn delete(&self, entity: &str, id: &str) -> Result<Response>;
    async fn delete_all(&self, entity: &str) -> Result<Response>;
    async fn count(&self, entity: &str) -> Result<Response>;
    async fn exists(&self, entity: &str, id: &str) -> Result<Response>;

    // Query
    async fn query(&self, body: QueryBody) -> Result<Response>;

    // Schema
    async fn get_schema(&self, entity: &str) -> Result<Response>;
    async fn set_schema(&self, entity: &str, body: Value) -> Result<Response>;
    async fn create_entity_type(&self, entity: &str, body: Value) -> Result<Response>;
    async fn list_entity_types(&self) -> Result<Response>;

    // Auth
    async fn login(&self, username: &str, password: &str) -> Result<Response>;
    async fn logout(&self) -> Result<Response>;
    async fn me(&self) -> Result<Response>;
    async fn create_user(&self, body: Value) -> Result<Response>;
    async fn list_users(&self) -> Result<Response>;
    async fn get_user(&self, username: &str) -> Result<Response>;
    async fn delete_user(&self, username: &str) -> Result<Response>;
    async fn change_password(&self, username: &str, body: Value) -> Result<Response>;
    async fn change_role(&self, username: &str, body: Value) -> Result<Response>;

    // ACL
    async fn get_acl(&self, username: &str, entity: &str) -> Result<Response>;
    async fn get_all_acls(&self, username: &str) -> Result<Response>;
    async fn set_acl(&self, username: &str, entity: &str, body: Value) -> Result<Response>;
    async fn reset_acl(&self, username: &str, entity: &str) -> Result<Response>;

    // Transactions
    async fn tx_begin(&self) -> Result<Response>;
    async fn tx_write(&self, tx_id: &str, entity: &str, body: Value) -> Result<Response>;
    async fn tx_update(&self, tx_id: &str, entity: &str, id: &str, body: Value) -> Result<Response>;
    async fn tx_delete(&self, tx_id: &str, entity: &str, id: &str) -> Result<Response>;
    async fn tx_commit(&self, tx_id: &str) -> Result<Response>;
    async fn tx_rollback(&self, tx_id: &str) -> Result<Response>;

    // KV
    async fn kv_set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<Response>;
    async fn kv_get(&self, key: &str) -> Result<Response>;
    async fn kv_mget(&self, keys: &[&str]) -> Result<Response>;
    async fn kv_delete(&self, key: &str) -> Result<Response>;

    // Hooks
    async fn create_hook(&self, entity: &str, body: Value) -> Result<Response>;
    async fn list_hooks(&self, entity: &str) -> Result<Response>;
    async fn get_hook(&self, id: &str) -> Result<Response>;
    async fn update_hook(&self, id: &str, body: Value) -> Result<Response>;
    async fn delete_hook(&self, entity: &str, id: &str) -> Result<Response>;

    // Migrations
    async fn migrate(&self, entity: &str, body: Value) -> Result<Response>;

    // Import/Export
    async fn export(&self) -> Result<Response>;
    async fn import(&self, body: Value) -> Result<Response>;

    // System
    async fn health(&self) -> Result<Response>;
    async fn stats(&self) -> Result<Response>;
    async fn config(&self) -> Result<Response>;
    async fn save(&self) -> Result<Response>;
    async fn reset(&self) -> Result<Response>;
}
```

### 7.9 `tcp_client.rs` — TCP Client

- Raw TCP connection using `tokio::net::TcpStream`.
- Send text commands, read line-delimited responses.
- Methods: `ping()`, `set()`, `set_ttl()`, `get()`, `mget()`, `del()`, `reset()`, `save()`.

### 7.10 `runner.rs` — Test Orchestrator

- Discovers and registers all test suites.
- Executes suites sequentially (isolation between suites via data reset).
- **Cleanup between suites**: `DELETE /api/{entity}` is called for each known `battle_*` entity to clear document data. `POST /reset` is intentionally **not** used here: in the targeted ElysianDB versions it wipes every KV key including the admin session and per-entity ACL grants, after which even an explicitly re-logged-in admin gets `403 Access denied` on documents they own. KV-specific cleanup must therefore live inside the suites that actually set KV keys (e.g. the KV suite), not in the global between-suite cleanup.
- Collects results (`TestResult { suite, name, status, duration, error }`).
- Feeds results to `report.rs`.
- The crash recovery suite is special: it kills and restarts the ElysianDB process via `instance.rs`, so it must run last (before performance).

### 7.11 `report.rs` — Reporting

- Aggregate results per suite.
- Output formats:
  - **Text** (default): colored terminal output with tables.
  - **JSON**: machine-readable report for CI.
- Write report file to `.battle/reports/<timestamp>.json`.
- Update `latest.json` symlink.

### 7.12 `suites/mod.rs` — Suite Trait

```rust
#[async_trait]
pub trait TestSuite {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    async fn setup(&self, client: &ElysianClient) -> Result<()>;
    async fn run(&self, client: &ElysianClient) -> Vec<TestResult>;
    async fn teardown(&self, client: &ElysianClient) -> Result<()>;
}
```

---

## 8. Test Suites — Detailed Scenarios

> Full scenario catalog in `doc/test-scenarios.md`. Summary below.

> **Authoritative test catalog**: `doc/test-scenarios.md` — the tables below are summaries.

### 8.1 Health & System (5 tests)

| # | Test | Method | Expected |
|---|------|--------|----------|
| 1 | Health check | `GET /health` | 200 |
| 2 | Stats endpoint | `GET /stats` | 200 + JSON with expected keys |
| 3 | Config endpoint | `GET /config` | 200 + matches generated config |
| 4 | Force save | `POST /save` | 200 |
| 5 | Version header | any request | `X-Elysian-Version` header present |

### 8.2 Entity CRUD (24 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Create single entity | 200, returns object with auto `id` |
| 2 | Create with custom ID | 200, preserves given `id` |
| 3 | Create batch (array) | 200, each item gets `id` |
| 4 | Create empty object | 200 or 400 (validate behavior) |
| 5 | Create invalid JSON | 400 |
| 6 | List empty collection | 200, empty array |
| 7 | List returns all documents | 200, array length matches |
| 8 | List with limit | returns exactly `limit` items |
| 9 | List with offset | skips `offset` items |
| 10 | List with limit+offset | correct pagination window |
| 11 | List sorted ascending | ascending order verified |
| 12 | List sorted descending | descending order verified |
| 13 | List with field projection | only requested fields returned |
| 14 | List with search | full-text match works |
| 15 | Get by ID | 200, correct document |
| 16 | Get by ID — not found | 404 |
| 17 | Update single field | 200, field updated, others unchanged |
| 18 | Update nested field | 200, nested path updated |
| 19 | Batch update | 200, all items updated |
| 20 | Delete by ID | 200, subsequent GET returns 404 |
| 21 | Delete all | 200, list returns empty |
| 22 | Count | correct count after inserts/deletes |
| 23 | Exists — true | 200, truthy response |
| 24 | Exists — false | 404 or falsy response |

### 8.3 Query API (20 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Simple eq filter | correct results |
| 2 | neq filter | excludes matched |
| 3 | lt filter (numeric) | correct comparison |
| 4 | lte filter | correct comparison |
| 5 | gt filter | correct comparison |
| 6 | gte filter | correct comparison |
| 7 | contains (string) | substring match |
| 8 | contains (array) | array element match |
| 9 | not_contains | exclusion works |
| 10 | all operator | all values present |
| 11 | any operator | any value present |
| 12 | none operator | no values present |
| 13 | Glob pattern `*` | wildcard matching |
| 14 | AND compound filter | intersection |
| 15 | OR filter | union |
| 16 | Nested AND/OR | deep logical tree |
| 17 | Sort + filter + limit | combined params |
| 18 | countOnly=true | returns `{"count": N}` |
| 19 | Nested field filter | matches nested path |
| 20 | Empty result set | 200, empty array |

### 8.4 Query Parameters (URL-based) (8 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | `filter[field][eq]=value` | correct filtering |
| 2 | `filter[field][gt]=value` | correct filtering |
| 3 | `sort[field]=asc` | sorted results |
| 4 | `sort[field]=desc` | sorted results |
| 5 | `fields=f1,f2` | projection works |
| 6 | `search=term` | full-text results |
| 7 | `countOnly=true` | count response |
| 8 | Combined params | all params together |

### 8.5 Nested Entities (6 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Create with `@entity` | sub-entity created, reference stored |
| 2 | Deep nesting (3 levels) | recursive creation |
| 3 | `@entity` with existing ID | links to existing |
| 4 | `includes` expands nested | full object returned |
| 5 | `includes=all` | recursive expansion |
| 6 | Array of nested entities | all sub-entities created |

### 8.6 Schema (10 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Auto-inferred schema | schema exists after first insert |
| 2 | Get schema | returns field definitions |
| 3 | Type mismatch rejected | 400 on wrong type |
| 4 | Set manual schema | `_manual: true` |
| 5 | Strict mode rejects new fields | 400 (see note below) |
| 6 | Required field enforcement | 400 when missing |
| 7 | Create entity type (shorthand) | type created |
| 8 | Create entity type (full) | type with required flags |
| 9 | List entity types | all types returned |
| 10 | List entity type names | names only |

> **Note on S-05 (strict mode):** The global config starts with `api.schema.strict: false`. Strict enforcement is activated per-entity by setting a manual schema via `PUT /api/{entity}/schema` with explicit field definitions. Once an entity has `_manual: true`, only declared fields are accepted for that entity. The test creates a manual schema, then attempts to insert a document with an undeclared field — expecting 400.

### 8.7 Authentication (15 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Unauthenticated request | 401 |
| 2 | Token auth (valid) | 200 |
| 3 | Token auth (invalid) | 401 |
| 4 | Login (admin/admin) | 200 + session cookie |
| 5 | Session cookie auth | subsequent requests succeed |
| 6 | Get /me | returns current user |
| 7 | Create user | user created |
| 8 | List users | all users returned |
| 9 | Get user by name | returns user info |
| 10 | Change password | login with new password works |
| 11 | Change role | role updated |
| 12 | Logout | session invalidated |
| 13 | Delete user | user removed |
| 14 | Cannot delete default admin | rejected (400 or 403) |
| 15 | Login wrong password | 401 |

### 8.8 ACL (10 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Admin has full access | all operations succeed |
| 2 | User default: can create | owning_write succeeds |
| 3 | User default: can read own | owning_read succeeds |
| 4 | User default: cannot read others | admin's doc filtered out |
| 5 | Grant global read | user can read all |
| 6 | Get ACL | returns permission set |
| 7 | Get all ACLs | returns all entity ACLs |
| 8 | Revoke permission | access denied |
| 9 | Reset ACL to default | owning permissions only |
| 10 | User cannot delete others' doc | rejected (403) |

### 8.9 Transactions (8 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Begin transaction | returns `transaction_id` |
| 2 | Write in transaction | accepted |
| 3 | Update in transaction | accepted |
| 4 | Delete in transaction | accepted |
| 5 | Commit | all operations applied |
| 6 | Rollback | no operations applied |
| 7 | Read during transaction (isolation) | uncommitted data not visible |
| 8 | Invalid transaction ID | error |

### 8.10 KV Store (8 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Set and get | value returned |
| 2 | Set with TTL | value expires |
| 3 | Get non-existent key | empty or 404 |
| 4 | Multi-get | multiple values |
| 5 | Delete key | removed |
| 6 | Overwrite key | new value |
| 7 | Large value (100KB) | handles big payload |
| 8 | Special characters in key | works correctly |

### 8.11 TCP Protocol (8 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | PING | PONG |
| 2 | SET / GET | value stored and retrieved |
| 3 | SET TTL / wait / GET | expired |
| 4 | MGET | multiple values |
| 5 | DEL | deleted |
| 6 | RESET | all keys cleared |
| 7 | SAVE | flush acknowledged |
| 8 | Invalid command | error response |

### 8.12 Import/Export (4 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Export empty DB | empty JSON |
| 2 | Export with data | all entities in dump |
| 3 | Import valid data | data restored |
| 4 | Round-trip (export → reset → import) | data identical |

### 8.13 Hooks (7 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Create pre_read hook | hook created |
| 2 | pre_read adds virtual field | field present in GET |
| 3 | Create post_read hook | hook created |
| 4 | post_read enrichment (ctx.query) | cross-entity data present |
| 5 | List hooks | returns created hooks |
| 6 | Disable hook | virtual field absent |
| 7 | Delete hook | hook removed |

### 8.14 Migrations (3 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Set field on all documents | field updated globally |
| 2 | Set nested path | nested field updated |
| 3 | Multiple set actions | all applied |

### 8.15 Edge Cases (12 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Unicode in values | preserved correctly |
| 2 | Unicode in field names | preserved correctly |
| 3 | Empty string values | stored and returned |
| 4 | Very long string (100KB) | accepted or clear limit |
| 5 | Deeply nested object (10 levels) | stored correctly |
| 6 | Large array (1000 items) | handled |
| 7 | Concurrent creates (50 parallel) | no data corruption |
| 8 | Duplicate custom ID | error or idempotent |
| 9 | Boolean and null values | types preserved |
| 10 | Numeric precision | values not truncated |
| 11 | Empty array value | stored as empty array |
| 12 | Request with trailing slash | same behavior as without |

### 8.16 Crash Recovery (3 tests)

| # | Test | Expected |
|---|------|----------|
| 1 | Kill + restart: data survives | SIGKILL ElysianDB, restart, verify previously saved data intact |
| 2 | Kill during writes: WAL replay | Insert data, force save, insert more, SIGKILL, restart, verify WAL-recovered data |
| 3 | Corrupted data dir recovery | Remove a shard file, restart, verify server starts and remaining data accessible |

> Crash recovery tests require stopping and restarting the ElysianDB instance. The runner handles this by using `instance.rs` to kill (SIGKILL, not SIGTERM) and relaunch the process.

### 8.17 Performance (8 scenarios)

| # | Scenario | Measurement |
|---|----------|-------------|
| 1 | Single document create | latency p50/p95/p99 |
| 2 | Batch create (100 documents) | latency + throughput |
| 3 | Single get by ID | latency p50/p95/p99 |
| 4 | List with 1,000 documents | latency p50/p95/p99 |
| 5 | Filtered query on 1,000 documents | latency p50/p95/p99 |
| 6 | Sorted query | latency p50/p95/p99 |
| 7 | Concurrent reads (10 parallel) | throughput (req/s), latency p99 |
| 8 | KV set/get cycle | latency p50/p95/p99 |

Performance tests run each scenario N times (configurable, default 100–500 iterations), compute percentiles, and report in the final summary.

---

## 9. ElysianDB Configuration Strategy

The test harness generates a **tailored `elysian.yaml`** for maximum test coverage:

```yaml
store:
  folder: ".battle/data"
  shards: 64                    # Lower for testing, faster startup
  flushIntervalSeconds: 30      # Long interval — we flush manually
  crashRecovery:
    enabled: true
    maxLogMB: 50

engine:
  name: "internal"

server:
  http:
    enabled: true
    host: "127.0.0.1"
    port: <auto-selected>
  tcp:
    enabled: true
    host: "127.0.0.1"
    port: <auto-selected>

log:
  flushIntervalSeconds: 5

stats:
  enabled: true

security:
  authentication:
    enabled: true
    mode: "user"
    token: "battle-test-token-2026"

api:
  schema:
    enabled: true
    strict: false
  index:
    workers: 2
  cache:
    enabled: true
    cleanupIntervalSeconds: 5
  hooks:
    enabled: true

adminui:
  enabled: false
```

Key decisions:
- **`mode: "user"`** — enables the most features (session auth, ACL, user management). Token mode is also tested by switching the `Authorization` header.
- **`host: 127.0.0.1`** — only listen on loopback, no exposure.
- **`adminui: false`** — no browser testing in v0.1.
- **`stats: true`** — needed for performance reporting.

---

## 10. Rust Dependencies (Cargo)

| Crate | Purpose | Version guidance |
|-------|---------|-----------------|
| `tokio` | Async runtime | latest stable, features: full |
| `reqwest` | HTTP client | latest, features: json, cookies |
| `serde` / `serde_json` | JSON ser/de | latest |
| `serde_yaml` | YAML generation for config | latest |
| `clap` | CLI argument parsing | v4, features: derive |
| `dialoguer` | Interactive terminal prompts | latest |
| `console` | Terminal colors & styling | latest |
| `indicatif` | Progress bars | latest |
| `tokio::net` | TCP client (built into tokio) | — |
| `chrono` | Timestamps for reports | latest |
| `uuid` | Generate test data IDs | latest, features: v4 |
| `anyhow` | Error handling | latest |
| `tracing` / `tracing-subscriber` | Structured logging | latest |
| `tabled` | Terminal table rendering | latest |

---

## 11. Error Handling Strategy

- **Prerequisite failures** (no Go, no Git): print clear error with install instructions, exit 1.
- **Git failures** (network, bad ref): print error, suggest checking ref name, exit 1.
- **Build failures**: capture and display `go build` stderr verbatim, exit 1.
- **Startup failures** (health check timeout): print last 50 lines of ElysianDB log, exit 1.
- **Test failures**: never abort — collect all results, report at the end.
- **Unexpected panics**: caught by Rust's panic handler, report partial results.

---

## 12. Lifecycle & Cleanup

1. **Before tests**: wipe `.battle/data/` to ensure clean state.
2. **Between suites**: call `DELETE /api/{entity}` for every known `battle_*` entity to isolate suites. `POST /reset` is intentionally excluded because it destroys admin ACL state for the rest of the process — KV-specific cleanup is performed inside the suites that own the keys.
3. **After tests**: stop ElysianDB (SIGTERM), optionally keep alive (`--keep-alive`).
4. **Reports**: never deleted automatically, accumulate in `.battle/reports/`.

---

## 13. CI Integration

Elysian-battle is designed to run in CI pipelines:

```yaml
# Example GitHub Actions step
- name: Run elysian-battle
  run: |
    cargo build --release
    ./target/release/elysian-battle \
      --version ${{ github.head_ref || 'main' }} \
      --report json \
      --verbose
```

Exit codes:
- `0`: all tests passed
- `1`: one or more tests failed
- `2`: infrastructure error (build, startup, prerequisites)

---

## 14. Future Phases (out of scope for v0.1)

| Phase | Scope |
|-------|-------|
| v0.2 | MongoDB engine testing (spin up MongoDB via Docker) |
| v0.3 | ElysianGate cluster testing |
| v0.4 | Admin UI smoke tests (headless browser) |
| v0.5 | Comparative benchmarks between versions |
| v0.6 | Webhook/event-driven test triggers |

---

## 15. Open Questions

1. **Should elysian-battle support testing a pre-built binary** (e.g., Docker image) instead of compiling from source? → Deferred to v0.2.
2. **Should we test crash recovery** (kill -9 + restart + verify data)? → Yes, added as dedicated suite `crash_recovery` (section 8.16) in v0.1.
3. **Should authentication mode be switchable per suite** (restart ElysianDB with different config)? → Desirable but complex. v0.1 uses `user` mode only, tests token auth via header swap.
