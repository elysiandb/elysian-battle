//! Suite 13 — Hooks (7 tests, HK-01..HK-07).
//!
//! Exercises ElysianDB's per-entity JavaScript hook pipeline:
//!
//!   - `POST /api/hook/{entity}` creates a hook bound to `{entity}` for
//!     either the `pre_read` or `post_read` event.
//!   - `GET  /api/hook/{entity}` returns every hook registered for the
//!     entity.
//!   - `PUT  /api/hook/id/{id}` updates a hook — but because the controller
//!     rebuilds a `hook.Hook` struct from the posted map and persists its
//!     full `ToDataMap()` output, any field missing from the payload is
//!     serialized as its Go zero value (empty string, false, 0). We work
//!     around this by fetching the hook first, merging the change locally,
//!     and PUT'ing the full body back.
//!   - `DELETE /api/hook/{entity}/{id}` removes a hook.
//!
//! Hooks receive a `ctx` with `entity` (the current document) and `query`
//! (a function that dispatches to `engine.ListEntities`) —
//! `internal/hook/executor.go`. Mutating `ctx.entity` inside the script is
//! visible on the response because the map is passed by reference.
//!
//! ## Where hooks fire
//!
//! `pre_read` hooks fire inside `ListController` and `QueryController`
//! (`internal/transport/http/api/list.go`), NOT inside
//! `GetByIdController`. `post_read` hooks fire in all three. The ticket's
//! HK-02 says "GET" but the only read path that triggers `pre_read` in the
//! current ElysianDB is a list/query call, so HK-02 verifies via
//! `GET /api/battle_hooked_entity` (the list endpoint).
//!
//! ## Auth requirement
//!
//! Hook management endpoints sit behind `AdminAuth` — the harness's default
//! session is already logged in as `admin`, so no extra login is needed.

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const ENTITY: &str = "battle_hooked_entity";
const RELATED: &str = "battle_hook_related";
const SEED_ID: &str = "hk-seed-1";

/// Inline JS sources kept near the tests that use them — avoids the need to
/// jump between files when validating hook payloads.
const PRE_READ_SCRIPT: &str = "function preRead(ctx) { ctx.entity.isOld = true; }";
const POST_READ_SCRIPT: &str = "function postRead(ctx) { \
    var rel = ctx.query('battle_hook_related', { group: { eq: 'A' } }); \
    ctx.entity.relatedCount = rel.length; \
}";

pub struct HooksSuite;

#[async_trait]
impl TestSuite for HooksSuite {
    fn name(&self) -> &'static str {
        "Hooks"
    }

    fn description(&self) -> &'static str {
        "Validates pre_read/post_read hook creation, application via list/get, listing, disabling, and deletion"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        let _ = client.delete_all(ENTITY).await;
        let _ = client.delete_all(RELATED).await;
        delete_all_hooks(client, ENTITY).await;

        // Seed one hooked document and two related docs so post_read's
        // ctx.query has predictable input.
        let _ = client
            .create(
                ENTITY,
                json!({"id": SEED_ID, "title": "Hooked", "state": "active"}),
            )
            .await;
        let _ = client
            .create(
                RELATED,
                json!({"id": "rel-a1", "group": "A", "label": "a1"}),
            )
            .await;
        let _ = client
            .create(
                RELATED,
                json!({"id": "rel-a2", "group": "A", "label": "a2"}),
            )
            .await;
        let _ = client
            .create(
                RELATED,
                json!({"id": "rel-b1", "group": "B", "label": "b1"}),
            )
            .await;
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(7);

        results.push(hk01_create_pre_read(&suite, client).await);
        results.push(hk02_pre_read_virtual_field(&suite, client).await);
        results.push(hk03_create_post_read(&suite, client).await);
        results.push(hk04_post_read_enrichment(&suite, client).await);
        results.push(hk05_list_hooks(&suite, client).await);
        results.push(hk06_disable_hook(&suite, client).await);
        results.push(hk07_delete_hook(&suite, client).await);

        results
    }

    async fn teardown(&self, client: &ElysianClient) -> Result<()> {
        delete_all_hooks(client, ENTITY).await;
        let _ = client.delete_all(ENTITY).await;
        let _ = client.delete_all(RELATED).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Best-effort: list every hook attached to `entity` and delete each one.
/// Used by setup/teardown so the suite always runs against a clean slate.
async fn delete_all_hooks(client: &ElysianClient, entity: &str) {
    let hooks = match list_hooks_raw(client, entity).await {
        Ok(list) => list,
        Err(_) => return,
    };
    for hook in hooks {
        if let Some(id) = hook.get("id").and_then(|v| v.as_str()) {
            let _ = client.delete_hook(entity, id).await;
        }
    }
}

/// Fetch `/api/hook/{entity}` and decode it into a JSON array. Returns an
/// error string so test bodies can fold it into `fail(...)` directly.
async fn list_hooks_raw(client: &ElysianClient, entity: &str) -> Result<Vec<Value>, String> {
    let resp = client
        .list_hooks(entity)
        .await
        .map_err(|e| format!("list hooks request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("list hooks expected 200, got {status}"));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("list hooks JSON parse failed: {e:#}"))?;
    match body {
        Value::Array(arr) => Ok(arr),
        Value::Null => Ok(Vec::new()),
        other => Err(format!("list hooks expected array, got {other}")),
    }
}

/// Create a hook for `entity` and return the generated hook id. The hook
/// payload is built from `event` + `script`; other fields fall back to the
/// server defaults (`language` is required by the schema so we set it).
///
/// ## Why we look the id up after create
///
/// `CreateHookForEntityController` unmarshals the posted body into a
/// `hook.Hook` value, passes that value (by copy!) to `hook.CreateHook`
/// which generates a UUID and persists it, then marshals the *original*
/// caller-side struct as the response — so the response body's `id` is the
/// empty string we posted, not the UUID that got persisted. We accept any
/// 200 from the POST and then resolve the real id by listing hooks on the
/// entity and matching our `name`.
async fn create_hook(
    client: &ElysianClient,
    entity: &str,
    name: &str,
    event: &str,
    script: &str,
) -> Result<String, String> {
    let body = json!({
        "name": name,
        "event": event,
        "language": "javascript",
        "script": script,
        "priority": 10,
        "bypass_acl": false,
        "enabled": true,
    });
    let resp = client
        .create_hook(entity, body)
        .await
        .map_err(|e| format!("create hook request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("create hook expected 200, got {status}"));
    }

    // Drain the response body so the HTTP connection is free for the
    // follow-up list — we intentionally discard its `id` field.
    let _ = resp.bytes().await;

    let hooks = list_hooks_raw(client, entity).await?;
    let found = hooks
        .iter()
        .find(|h| h.get("name").and_then(|v| v.as_str()) == Some(name))
        .ok_or_else(|| format!("hook `{name}` not found after create"))?;
    let id = found
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("hook `{name}` missing id: {found}"))?;
    if id.is_empty() {
        return Err(format!("hook `{name}` has empty id after create"));
    }
    Ok(id.to_string())
}

/// Find the first hook for `entity` whose `event` field matches `event`.
async fn find_hook_by_event(
    client: &ElysianClient,
    entity: &str,
    event: &str,
) -> Result<Value, String> {
    let hooks = list_hooks_raw(client, entity).await?;
    hooks
        .into_iter()
        .find(|h| h.get("event").and_then(|v| v.as_str()) == Some(event))
        .ok_or_else(|| format!("no `{event}` hook found for entity `{entity}`"))
}

/// LIST `/api/{entity}` (no filters), decode to array, and return the doc
/// whose `id` matches `want_id`. Test helper for verifying pre_read hook
/// side effects — pre_read fires on LIST, not on GET-by-id.
async fn list_and_find(
    client: &ElysianClient,
    entity: &str,
    want_id: &str,
) -> Result<Value, String> {
    let resp = client
        .list(entity, &[])
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
    arr.iter()
        .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(want_id))
        .cloned()
        .ok_or_else(|| format!("doc `{want_id}` missing from list of {}", arr.len()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// HK-01 — Creating a pre_read hook returns 200 and a non-empty id.
async fn hk01_create_pre_read(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "HK-01 Create pre_read hook";
    let request = format!("POST /api/hook/{ENTITY} (pre_read)");
    let start = Instant::now();

    match create_hook(
        client,
        ENTITY,
        "battle_pre_read_v1",
        "pre_read",
        PRE_READ_SCRIPT,
    )
    .await
    {
        Ok(_) => pass(suite, name, request, Some(200), start.elapsed()),
        Err(msg) => fail(suite, name, request, None, start.elapsed(), msg),
    }
}

// HK-02 — The pre_read hook adds `isOld: true` to documents returned via
// the list endpoint. GET-by-id is intentionally avoided: the ElysianDB
// controller only fires post_read on that path
// (`internal/transport/http/api/get_by_id.go`).
async fn hk02_pre_read_virtual_field(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "HK-02 pre_read virtual field";
    let request = format!("GET /api/{ENTITY} (list, pre_read fires)");
    let start = Instant::now();

    let doc = match list_and_find(client, ENTITY, SEED_ID).await {
        Ok(d) => d,
        Err(msg) => return fail(suite, name, request, Some(200), start.elapsed(), msg),
    };
    let duration = start.elapsed();

    match doc.get("isOld").and_then(|v| v.as_bool()) {
        Some(true) => pass(suite, name, request, Some(200), duration),
        _ => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected `isOld: true`, got doc {doc}"),
        ),
    }
}

// HK-03 — Creating a post_read hook returns 200 and a non-empty id.
async fn hk03_create_post_read(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "HK-03 Create post_read hook";
    let request = format!("POST /api/hook/{ENTITY} (post_read)");
    let start = Instant::now();

    match create_hook(
        client,
        ENTITY,
        "battle_post_read_v1",
        "post_read",
        POST_READ_SCRIPT,
    )
    .await
    {
        Ok(_) => pass(suite, name, request, Some(200), start.elapsed()),
        Err(msg) => fail(suite, name, request, None, start.elapsed(), msg),
    }
}

// HK-04 — The post_read hook uses `ctx.query` to count related docs in
// `battle_hook_related` where `group = "A"` and writes the count onto the
// response as `relatedCount`. Setup seeds 2 group="A" docs and 1 group="B",
// so we expect `relatedCount == 2`.
async fn hk04_post_read_enrichment(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "HK-04 post_read enrichment";
    let request = format!("GET /api/{ENTITY}/{SEED_ID}");
    let start = Instant::now();

    let resp = match client.get(ENTITY, SEED_ID).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("get request failed: {e:#}"),
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
            format!("expected 200, got {status}"),
        );
    }
    let doc: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("JSON parse failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let count = doc.get("relatedCount").and_then(|v| v.as_i64());
    match count {
        Some(2) => pass(suite, name, request, Some(status), duration),
        other => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected relatedCount=2, got {other:?} in doc {doc}"),
        ),
    }
}

// HK-05 — `GET /api/hook/{entity}` returns both hooks created above.
async fn hk05_list_hooks(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "HK-05 List hooks";
    let request = format!("GET /api/hook/{ENTITY}");
    let start = Instant::now();

    let hooks = match list_hooks_raw(client, ENTITY).await {
        Ok(list) => list,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let has_pre = hooks
        .iter()
        .any(|h| h.get("event").and_then(|v| v.as_str()) == Some("pre_read"));
    let has_post = hooks
        .iter()
        .any(|h| h.get("event").and_then(|v| v.as_str()) == Some("post_read"));

    if hooks.len() >= 2 && has_pre && has_post {
        pass(suite, name, request, Some(200), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!(
                "expected at least one pre_read + one post_read, got {} hooks (pre={has_pre}, post={has_post})",
                hooks.len()
            ),
        )
    }
}

// HK-06 — Disable the pre_read hook and confirm `isOld` no longer appears on
// the list response.
//
// UpdateHookByIdController rebuilds a full Hook struct from the PUT body
// and persists every field via `ToDataMap()` — any field missing from the
// payload is replaced by its Go zero value. We therefore fetch the current
// hook, flip `enabled`, and send the whole object back.
async fn hk06_disable_hook(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "HK-06 Disable hook";
    let request = "PUT /api/hook/id/{id} (enabled=false)".to_string();
    let start = Instant::now();

    let mut current = match find_hook_by_event(client, ENTITY, "pre_read").await {
        Ok(h) => h,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let hook_id = current
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();
    if hook_id.is_empty() {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre_read hook has empty id: {current}"),
        );
    }
    if let Some(obj) = current.as_object_mut() {
        obj.insert("enabled".to_string(), json!(false));
    }

    let resp = match client.update_hook(&hook_id, current).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("update hook failed: {e:#}"),
            )
        }
    };
    let update_status = resp.status().as_u16();
    if update_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(update_status),
            start.elapsed(),
            format!("update expected 200, got {update_status}"),
        );
    }

    let doc = match list_and_find(client, ENTITY, SEED_ID).await {
        Ok(d) => d,
        Err(msg) => {
            return fail(
                suite,
                name,
                request,
                Some(update_status),
                start.elapsed(),
                msg,
            )
        }
    };
    let duration = start.elapsed();

    if doc.get("isOld").is_none() {
        pass(suite, name, request, Some(update_status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(update_status),
            duration,
            format!("expected `isOld` absent after disabling hook, got doc {doc}"),
        )
    }
}

// HK-07 — Delete the (disabled) pre_read hook and confirm it no longer
// appears in the entity's hook list.
async fn hk07_delete_hook(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "HK-07 Delete hook";
    let request = format!("DELETE /api/hook/{ENTITY}/{{id}}");
    let start = Instant::now();

    let hook = match find_hook_by_event(client, ENTITY, "pre_read").await {
        Ok(h) => h,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let hook_id = hook
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();
    if hook_id.is_empty() {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre_read hook has empty id: {hook}"),
        );
    }

    let resp = match client.delete_hook(ENTITY, &hook_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("delete hook request failed: {e:#}"),
            )
        }
    };
    let delete_status = resp.status().as_u16();
    if delete_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(delete_status),
            start.elapsed(),
            format!("delete expected 200, got {delete_status}"),
        );
    }

    // Verify the pre_read hook is gone; post_read should still be present.
    let hooks = match list_hooks_raw(client, ENTITY).await {
        Ok(list) => list,
        Err(msg) => {
            return fail(
                suite,
                name,
                request,
                Some(delete_status),
                start.elapsed(),
                msg,
            )
        }
    };
    let duration = start.elapsed();

    let still_has_pre = hooks
        .iter()
        .any(|h| h.get("event").and_then(|v| v.as_str()) == Some("pre_read"));

    if still_has_pre {
        fail(
            suite,
            name,
            request,
            Some(delete_status),
            duration,
            "pre_read hook still present after DELETE".to_string(),
        )
    } else {
        pass(suite, name, request, Some(delete_status), duration)
    }
}
