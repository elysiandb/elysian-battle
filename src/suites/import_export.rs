//! Suite 12 — Import / Export (4 tests, IE-01..IE-04).
//!
//! Exercises ElysianDB's full-database dump endpoints:
//!
//!   - `GET /api/export` returns a JSON object `{ "<entity>": [docs...], ... }`
//!     containing every entity type known to the engine (minus the internal
//!     schema entity). The controller always responds 200 on a valid engine
//!     (`internal/transport/http/api/export.go`).
//!   - `POST /api/import` accepts the same shape, and for every entity key
//!     present in the payload it wipes that entity's existing documents and
//!     rewrites them from the payload (`internal/api/storage.go:ImportAll`).
//!     Entities NOT mentioned in the payload are untouched — so we can do
//!     round-trips on a single test entity without disturbing users, hooks,
//!     or ACLs.
//!
//! ## Test strategy
//!
//! All four tests operate on `battle_export_test` and treat the rest of the
//! DB as opaque. IE-04's "reset" step is a `DELETE /api/battle_export_test`
//! rather than `POST /reset` — the latter would wipe the admin session and
//! every ACL grant, which cascades into the remaining suites (see the KV
//! suite's cache/cleanup notes for the full rationale).

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const ENTITY: &str = "battle_export_test";

pub struct ImportExportSuite;

#[async_trait]
impl TestSuite for ImportExportSuite {
    fn name(&self) -> &'static str {
        "Import Export"
    }

    fn description(&self) -> &'static str {
        "Validates full-database export, import, and round-trip integrity on a scoped test entity"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        let _ = client.delete_all(ENTITY).await;
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(4);

        results.push(ie01_export_empty(&suite, client).await);
        results.push(ie02_export_with_data(&suite, client).await);
        results.push(ie03_import_data(&suite, client).await);
        results.push(ie04_round_trip_integrity(&suite, client).await);

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

/// Fetch the full export dump and return it as a JSON object. Returns an
/// error string (not `anyhow::Error`) so test bodies can fold it straight
/// into a `fail(...)` result.
async fn export_dump(client: &ElysianClient) -> Result<Value, String> {
    let resp = client
        .export()
        .await
        .map_err(|e| format!("export request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("export expected 200, got {status}"));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| format!("export JSON parse failed: {e:#}"))
}

/// Extract the array stored at `dump[entity]`, returning an empty array if
/// the key is missing. The export omits entity types that have never been
/// written to, so an absent key is equivalent to an empty list.
fn entity_docs<'a>(dump: &'a Value, entity: &str) -> &'a [Value] {
    // Shared empty Vec so the missing-key branch hands back a `&[Value]`
    // without a per-call allocation; `Vec::new()` is const-constructible.
    static EMPTY: Vec<Value> = Vec::new();
    dump.get(entity)
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&EMPTY)
}

/// Sort a slice of documents by their `id` field for deterministic
/// comparison. Returns a `Vec<Value>` copy — callers are free to mutate.
fn sorted_by_id(docs: &[Value]) -> Vec<Value> {
    let mut owned: Vec<Value> = docs.to_vec();
    owned.sort_by(|a, b| {
        let ak = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let bk = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
        ak.cmp(bk)
    });
    owned
}

/// Seed `count` docs with predictable ids `ie-<tag>-<n>`. Returns the list
/// of ids seeded so tests can verify each one survived the round-trip.
async fn seed_docs(client: &ElysianClient, tag: &str, count: usize) -> Result<Vec<String>, String> {
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let id = format!("ie-{tag}-{i}");
        let body = json!({"id": id, "index": i, "label": format!("doc-{i}")});
        let resp = client
            .create(ENTITY, body)
            .await
            .map_err(|e| format!("seed create failed: {e:#}"))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(format!("seed create {id} expected 200, got {status}"));
        }
        ids.push(id);
    }
    Ok(ids)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// IE-01 — Export on an empty entity returns a valid JSON object.
//
// The spec says "Export empty database", but the harness shares a single
// ElysianDB instance across suites so the DB as a whole is never truly
// empty. We scope the assertion to our own entity: after `delete_all`, the
// export's `battle_export_test` key (if present) must be an empty array,
// or the key must be absent entirely.
async fn ie01_export_empty(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "IE-01 Export empty database";
    let request = "GET /api/export".to_string();
    let start = Instant::now();

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre-export wipe failed: {e:#}"),
        );
    }

    let dump = match export_dump(client).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    if !dump.is_object() {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected top-level JSON object, got {dump}"),
        );
    }

    let docs = entity_docs(&dump, ENTITY);
    if !docs.is_empty() {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected {ENTITY} empty, got {} docs", docs.len()),
        );
    }

    pass(suite, name, request, Some(200), duration)
}

// IE-02 — Export after seeding docs includes every seeded doc.
async fn ie02_export_with_data(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "IE-02 Export with data";
    let request = "GET /api/export".to_string();
    let start = Instant::now();

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre-seed wipe failed: {e:#}"),
        );
    }

    let seeded = match seed_docs(client, "02", 3).await {
        Ok(ids) => ids,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    let dump = match export_dump(client).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let docs = entity_docs(&dump, ENTITY);
    if docs.len() != seeded.len() {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!(
                "expected {} {ENTITY} docs in export, got {}",
                seeded.len(),
                docs.len()
            ),
        );
    }

    for id in &seeded {
        let found = docs
            .iter()
            .any(|d| d.get("id").and_then(|v| v.as_str()) == Some(id.as_str()));
        if !found {
            return fail(
                suite,
                name,
                request,
                Some(200),
                duration,
                format!("seeded id `{id}` missing from export"),
            );
        }
    }

    pass(suite, name, request, Some(200), duration)
}

// IE-03 — Import restores data previously wiped.
//
// Seed → export (to capture the wire shape) → delete each seeded doc by id
// → import (same payload, scoped to our entity) → verify the docs are
// readable via `GET /api/<entity>/<id>`.
//
// Note: we delete docs individually rather than via `DELETE /api/<entity>`.
// The entity-level destroy also calls `acl.DeleteACLForEntityType` which
// tears down admin's ACL row for the entity; `POST /api/import` writes
// docs directly via `engine.WriteEntity` and does NOT call
// `acl.InitACL()`, so after an entity-destroy the re-imported docs are
// invisible to admin (403 Forbidden on subsequent reads). Per-id deletes
// leave the ACL intact.
async fn ie03_import_data(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "IE-03 Import data";
    let request = "POST /api/import".to_string();
    let start = Instant::now();

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre-seed wipe failed: {e:#}"),
        );
    }

    let seeded = match seed_docs(client, "03", 4).await {
        Ok(ids) => ids,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    // Capture the exported shape for this entity so we feed the same bytes
    // back in on import.
    let dump = match export_dump(client).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let captured = entity_docs(&dump, ENTITY).to_vec();
    if captured.len() != seeded.len() {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!(
                "pre-import export mismatch: expected {} docs, got {}",
                seeded.len(),
                captured.len()
            ),
        );
    }

    // Remove each doc individually so the entity's ACL row survives.
    for id in &seeded {
        if let Err(e) = client.delete(ENTITY, id).await {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("pre-import delete {id} failed: {e:#}"),
            );
        }
    }

    let payload = json!({ ENTITY: captured });
    let resp = match client.import(payload).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("import request failed: {e:#}"),
            )
        }
    };
    let import_status = resp.status().as_u16();
    if import_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(import_status),
            start.elapsed(),
            format!("import expected 200, got {import_status}"),
        );
    }

    // Verify every seeded doc is readable post-import.
    for id in &seeded {
        let resp = match client.get(ENTITY, id).await {
            Ok(r) => r,
            Err(e) => {
                return fail(
                    suite,
                    name,
                    request,
                    Some(import_status),
                    start.elapsed(),
                    format!("post-import GET {id} failed: {e:#}"),
                )
            }
        };
        let status = resp.status().as_u16();
        if status != 200 {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("post-import GET {id} expected 200, got {status}"),
            );
        }
    }

    pass(suite, name, request, Some(import_status), start.elapsed())
}

// IE-04 — Round-trip integrity.
//
// Seed → export #1 → wipe → import (captured subset) → export #2 → compare
// the two `battle_export_test` slices after sorting by id. We compare by
// full JSON equality so the check catches field mutations in addition to
// missing/extra docs.
async fn ie04_round_trip_integrity(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "IE-04 Round-trip integrity";
    let request = "GET /api/export → POST /api/import → GET /api/export".to_string();
    let start = Instant::now();

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre-seed wipe failed: {e:#}"),
        );
    }

    if let Err(msg) = seed_docs(client, "04", 5).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let dump1 = match export_dump(client).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let first = sorted_by_id(entity_docs(&dump1, ENTITY));
    if first.is_empty() {
        return fail(
            suite,
            name,
            request,
            Some(200),
            start.elapsed(),
            "first export returned empty entity list".to_string(),
        );
    }

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("mid-round wipe failed: {e:#}"),
        );
    }

    let payload = json!({ ENTITY: first.clone() });
    let resp = match client.import(payload).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("import request failed: {e:#}"),
            )
        }
    };
    let import_status = resp.status().as_u16();
    if import_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(import_status),
            start.elapsed(),
            format!("import expected 200, got {import_status}"),
        );
    }

    let dump2 = match export_dump(client).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let second = sorted_by_id(entity_docs(&dump2, ENTITY));
    let duration = start.elapsed();

    if first != second {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!(
                "second export differs from first (first={} docs, second={} docs)",
                first.len(),
                second.len()
            ),
        );
    }

    pass(suite, name, request, Some(200), duration)
}
