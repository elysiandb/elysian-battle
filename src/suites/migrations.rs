//! Suite 14 — Migrations (3 tests, MG-01..MG-03).
//!
//! Exercises `POST /api/{entity}/migrate` — bulk set-field-on-every-document
//! operations driven by a JSON DSL
//! (`internal/api/migration.go:ParseMigrationQuery`).
//!
//! ## Request shape
//!
//! ```json
//! [
//!   {"set": [{"status": "migrated"}]},
//!   {"set": [{"metadata.version": "2.0"}]}
//! ]
//! ```
//!
//! Each top-level element is `{"<action>": <props>}`. Today only `set` is
//! supported (anything else returns 400). The payload can be either an
//! array-of-maps (`[{...}, {...}]`) or a single map — we use the array form
//! per the doc example.
//!
//! Dot-notation paths (`"metadata.migrationVersion"`) walk or create
//! nested maps via `SetNestedField` (`internal/api/field.go`), so MG-02
//! can add a field that the original seed schema never declared.
//!
//! ## Test strategy
//!
//! Setup seeds 10 docs with `status: "old"`. Each test re-seeds from
//! scratch so migrations don't cascade: MG-02 should see `status: "old"`
//! (not `status: "migrated"` left over from MG-01).
//!
//! ## Endpoint prerequisite
//!
//! The controller requires the engine to be `internal` and the entity
//! type to already exist (`engine.EntityTypeExists`). Seeding via
//! `POST /api/{entity}` registers the entity type as a side effect
//! (`AddEntityType` inside `persistEntity`), so the setup path covers
//! this without an explicit `/create` call.

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const ENTITY: &str = "battle_migrate_test";
const SEED_COUNT: usize = 10;

pub struct MigrationsSuite;

#[async_trait]
impl TestSuite for MigrationsSuite {
    fn name(&self) -> &'static str {
        "Migrations"
    }

    fn description(&self) -> &'static str {
        "Validates bulk migration set actions — flat field, nested path, multiple actions — across every document in an entity"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        let _ = client.delete_all(ENTITY).await;
        let _ = seed_old_docs(client).await;
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(3);

        results.push(mg01_set_field_on_all(&suite, client).await);
        results.push(mg02_set_nested_path(&suite, client).await);
        results.push(mg03_multiple_set_actions(&suite, client).await);

        results
    }

    async fn teardown(&self, client: &ElysianClient) -> Result<()> {
        let _ = client.delete_all(ENTITY).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Wipe then seed `SEED_COUNT` docs with predictable ids and `status: "old"`.
/// Used at the start of every test so migrations don't carry state across
/// MG-01/02/03.
async fn reset_and_seed(client: &ElysianClient) -> Result<(), String> {
    client
        .delete_all(ENTITY)
        .await
        .map_err(|e| format!("reset delete failed: {e:#}"))?;
    seed_old_docs(client).await
}

/// Seed `SEED_COUNT` docs. Kept as a standalone helper so setup() can call
/// it without going through the per-test reset path.
async fn seed_old_docs(client: &ElysianClient) -> Result<(), String> {
    for i in 0..SEED_COUNT {
        let id = format!("mg-seed-{i}");
        let body = json!({
            "id": id,
            "status": "old",
            "index": i,
        });
        let resp = client
            .create(ENTITY, body)
            .await
            .map_err(|e| format!("seed create failed: {e:#}"))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(format!("seed create {id} expected 200, got {status}"));
        }
    }
    Ok(())
}

/// List every doc in `ENTITY` and return them as a JSON array. Returns an
/// error string so test bodies can fold it into `fail(...)`.
async fn list_all(client: &ElysianClient) -> Result<Vec<Value>, String> {
    let resp = client
        .list(ENTITY, &[])
        .await
        .map_err(|e| format!("list request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("list expected 200, got {status}"));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("list JSON parse failed: {e:#}"))?;
    let arr = body
        .as_array()
        .ok_or_else(|| format!("list expected array, got {body}"))?;
    Ok(arr.clone())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// MG-01 — `set` flips `status: "old"` → `"migrated"` on every doc.
async fn mg01_set_field_on_all(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "MG-01 Set field on all";
    let request = format!("POST /api/{ENTITY}/migrate body=`[{{set:[{{status:migrated}}]}}]`");
    let start = Instant::now();

    if let Err(msg) = reset_and_seed(client).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let body = json!([{"set": [{"status": "migrated"}]}]);
    let resp = match client.migrate(ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("migrate request failed: {e:#}"),
            )
        }
    };
    let migrate_status = resp.status().as_u16();
    if migrate_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(migrate_status),
            start.elapsed(),
            format!("migrate expected 200, got {migrate_status}"),
        );
    }

    let docs = match list_all(client).await {
        Ok(d) => d,
        Err(msg) => {
            return fail(
                suite,
                name,
                request,
                Some(migrate_status),
                start.elapsed(),
                msg,
            )
        }
    };
    let duration = start.elapsed();

    if docs.len() != SEED_COUNT {
        return fail(
            suite,
            name,
            request,
            Some(migrate_status),
            duration,
            format!("expected {SEED_COUNT} docs, got {}", docs.len()),
        );
    }

    for doc in &docs {
        let status = doc.get("status").and_then(|v| v.as_str());
        if status != Some("migrated") {
            return fail(
                suite,
                name,
                request,
                Some(migrate_status),
                duration,
                format!("expected status=\"migrated\", got {status:?} in {doc}"),
            );
        }
    }

    pass(suite, name, request, Some(migrate_status), duration)
}

// MG-02 — Dotted path `metadata.migrationVersion` creates the nested map
// and sets the leaf value on every doc.
async fn mg02_set_nested_path(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "MG-02 Set nested path";
    let request =
        format!("POST /api/{ENTITY}/migrate body=`[{{set:[{{metadata.migrationVersion:2}}]}}]`");
    let start = Instant::now();

    if let Err(msg) = reset_and_seed(client).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let body = json!([{"set": [{"metadata.migrationVersion": "2"}]}]);
    let resp = match client.migrate(ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("migrate request failed: {e:#}"),
            )
        }
    };
    let migrate_status = resp.status().as_u16();
    if migrate_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(migrate_status),
            start.elapsed(),
            format!("migrate expected 200, got {migrate_status}"),
        );
    }

    let docs = match list_all(client).await {
        Ok(d) => d,
        Err(msg) => {
            return fail(
                suite,
                name,
                request,
                Some(migrate_status),
                start.elapsed(),
                msg,
            )
        }
    };
    let duration = start.elapsed();

    if docs.len() != SEED_COUNT {
        return fail(
            suite,
            name,
            request,
            Some(migrate_status),
            duration,
            format!("expected {SEED_COUNT} docs, got {}", docs.len()),
        );
    }

    for doc in &docs {
        let version = doc
            .get("metadata")
            .and_then(|m| m.get("migrationVersion"))
            .and_then(|v| v.as_str());
        if version != Some("2") {
            return fail(
                suite,
                name,
                request,
                Some(migrate_status),
                duration,
                format!("expected metadata.migrationVersion=\"2\", got {version:?} in {doc}"),
            );
        }
    }

    pass(suite, name, request, Some(migrate_status), duration)
}

// MG-03 — Multiple `set` actions in one request: both fields land on every
// doc after a single commit.
async fn mg03_multiple_set_actions(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "MG-03 Multiple set actions";
    let request =
        format!("POST /api/{ENTITY}/migrate body=`[{{set:[{{a:1}}]}},{{set:[{{b:2}}]}}]`");
    let start = Instant::now();

    if let Err(msg) = reset_and_seed(client).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let body = json!([
        {"set": [{"a": 1}]},
        {"set": [{"b": 2}]}
    ]);
    let resp = match client.migrate(ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("migrate request failed: {e:#}"),
            )
        }
    };
    let migrate_status = resp.status().as_u16();
    if migrate_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(migrate_status),
            start.elapsed(),
            format!("migrate expected 200, got {migrate_status}"),
        );
    }

    let docs = match list_all(client).await {
        Ok(d) => d,
        Err(msg) => {
            return fail(
                suite,
                name,
                request,
                Some(migrate_status),
                start.elapsed(),
                msg,
            )
        }
    };
    let duration = start.elapsed();

    if docs.len() != SEED_COUNT {
        return fail(
            suite,
            name,
            request,
            Some(migrate_status),
            duration,
            format!("expected {SEED_COUNT} docs, got {}", docs.len()),
        );
    }

    for doc in &docs {
        let a = doc.get("a").and_then(|v| v.as_i64());
        let b = doc.get("b").and_then(|v| v.as_i64());
        if a != Some(1) || b != Some(2) {
            return fail(
                suite,
                name,
                request,
                Some(migrate_status),
                duration,
                format!("expected a=1, b=2, got a={a:?}, b={b:?} in {doc}"),
            );
        }
    }

    pass(suite, name, request, Some(migrate_status), duration)
}
