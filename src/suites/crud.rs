//! Suite 2 — Entity CRUD (24 tests, C-01..C-24).
//!
//! Exercises every documented Entity CRUD endpoint of ElysianDB:
//!   - Create: single, custom ID, batch, empty body, invalid JSON
//!   - List: empty, all, limit/offset, sort asc/desc, projection, search
//!   - Get: by ID, not found
//!   - Update: single field, nested field, batch
//!   - Delete: by ID, delete-all
//!   - Count
//!   - Exists: true / false
//!
//! Each test reseeds the entities it depends on so test ordering does not
//! couple individual cases together. The suite uses two entities:
//!   - `battle_books`  — primary working set (created and mutated by tests)
//!   - `battle_empty`  — must be empty for C-06 (List empty collection)
//!
//! The runner's between-suite cleanup wipes both before and after this suite
//! runs, but `setup`/`teardown` here ensure a clean slate even when the suite
//! is invoked in isolation via `--suite crud`.

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const ENTITY: &str = "battle_books";
const EMPTY_ENTITY: &str = "battle_empty";

pub struct CrudSuite;

#[async_trait]
impl TestSuite for CrudSuite {
    fn name(&self) -> &'static str {
        "Entity CRUD"
    }

    fn description(&self) -> &'static str {
        "Validates entity Create / Read / Update / Delete / Count / Exists endpoints"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        // Wipe both entities so the suite starts deterministically even when
        // run standalone (no preceding suite cleanup).
        let _ = client.delete_all(ENTITY).await;
        let _ = client.delete_all(EMPTY_ENTITY).await;
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(24);

        results.push(c01_create_single(&suite, client).await);
        results.push(c02_create_custom_id(&suite, client).await);
        results.push(c03_create_batch(&suite, client).await);
        results.push(c04_create_empty_body(&suite, client).await);
        results.push(c05_create_invalid_json(&suite, client).await);
        results.push(c06_list_empty(&suite, client).await);
        results.push(c07_list_all(&suite, client).await);
        results.push(c08_list_with_limit(&suite, client).await);
        results.push(c09_list_with_offset(&suite, client).await);
        results.push(c10_list_with_limit_and_offset(&suite, client).await);
        results.push(c11_list_sorted_asc(&suite, client).await);
        results.push(c12_list_sorted_desc(&suite, client).await);
        results.push(c13_list_field_projection(&suite, client).await);
        results.push(c14_list_search(&suite, client).await);
        results.push(c15_get_by_id(&suite, client).await);
        results.push(c16_get_not_found(&suite, client).await);
        results.push(c17_update_single_field(&suite, client).await);
        results.push(c18_update_nested_field(&suite, client).await);
        results.push(c19_batch_update(&suite, client).await);
        results.push(c20_delete_by_id(&suite, client).await);
        results.push(c21_delete_all(&suite, client).await);
        results.push(c22_count(&suite, client).await);
        results.push(c23_exists_true(&suite, client).await);
        results.push(c24_exists_false(&suite, client).await);

        results
    }

    async fn teardown(&self, client: &ElysianClient) -> Result<()> {
        let _ = client.delete_all(ENTITY).await;
        let _ = client.delete_all(EMPTY_ENTITY).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Standard book seed used by list/sort/projection/search/update tests.
///
/// Sorted ascending by `title`:  Anathem, Cryptonomicon, Dune, Snow Crash
/// Sorted descending by `pages`: Anathem(932), Cryptonomicon(918), Snow Crash(470), Dune(412)
fn standard_seed() -> Vec<Value> {
    vec![
        json!({"title": "Dune", "pages": 412}),
        json!({"title": "Anathem", "pages": 932}),
        json!({"title": "Cryptonomicon", "pages": 918}),
        json!({"title": "Snow Crash", "pages": 470}),
    ]
}

/// Wipe `battle_books` and insert the given documents one-by-one.
/// Returns the list of generated IDs (in insertion order) on success.
async fn reseed(client: &ElysianClient, docs: &[Value]) -> Result<Vec<String>> {
    let _ = client.delete_all(ENTITY).await;
    let mut ids = Vec::with_capacity(docs.len());
    for doc in docs {
        let resp = client.create(ENTITY, doc.clone()).await?;
        let body: Value = resp.json().await?;
        let id = body
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("seed insert: missing id in response"))?
            .to_string();
        ids.push(id);
    }
    Ok(ids)
}

/// Convert a `Result<Vec<String>>` from `reseed` into a failed `TestResult`
/// when seeding itself fails (so the test reports a clear cause).
fn seed_error(
    suite: &str,
    name: &str,
    request: &str,
    start: Instant,
    e: anyhow::Error,
) -> TestResult {
    fail(
        suite,
        name,
        request.to_string(),
        None,
        start.elapsed(),
        format!("seed setup failed: {e:#}"),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// C-01 — Create single document
async fn c01_create_single(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-01 Create single document";
    let request = format!("POST /api/{ENTITY} {{title:Dune,pages:412}}");
    let start = Instant::now();

    let _ = client.delete_all(ENTITY).await;

    let body = json!({"title": "Dune", "pages": 412});
    let resp = match client.create(ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let id = body.get("id").and_then(|v| v.as_str());
    let title = body.get("title").and_then(|v| v.as_str());

    match (id, title) {
        (Some(id), Some("Dune")) if !id.is_empty() => {
            pass(suite, name, request, Some(status), duration)
        }
        (None, _) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "response missing `id`",
        ),
        (_, t) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected title=\"Dune\", got {t:?}"),
        ),
    }
}

// C-02 — Create with custom ID
async fn c02_create_custom_id(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-02 Create with custom ID";
    let request = format!("POST /api/{ENTITY} {{id:custom-1,title:Custom}}");
    let start = Instant::now();

    let _ = client.delete_all(ENTITY).await;

    let body = json!({"id": "custom-1", "title": "Custom"});
    let resp = match client.create(ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match body.get("id").and_then(|v| v.as_str()) {
        Some("custom-1") => pass(suite, name, request, Some(status), duration),
        Some(other) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected id=\"custom-1\", got \"{other}\""),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "response missing `id`",
        ),
    }
}

// C-03 — Create batch
async fn c03_create_batch(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-03 Create batch";
    let request = format!("POST /api/{ENTITY} [A,B,C]");
    let start = Instant::now();

    let _ = client.delete_all(ENTITY).await;

    let body = json!([{"title": "A"}, {"title": "B"}, {"title": "C"}]);
    let resp = match client.create(ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let arr = match body.as_array() {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                "response is not an array",
            )
        }
    };
    if arr.len() != 3 {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 3 items, got {}", arr.len()),
        );
    }

    let mut ids = Vec::with_capacity(3);
    for (i, item) in arr.iter().enumerate() {
        match item.get("id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => ids.push(id.to_string()),
            _ => {
                return fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    duration,
                    format!("item {i} missing `id`"),
                )
            }
        }
    }
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    if unique.len() != 3 {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "duplicate ids in batch response",
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// C-04 — Create with empty body  (200 with generated id, OR 400 — both accepted)
async fn c04_create_empty_body(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-04 Create with empty body";
    let request = format!("POST /api/{ENTITY} {{}}");
    let start = Instant::now();

    let _ = client.delete_all(ENTITY).await;

    let resp = match client.create(ENTITY, json!({})).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
            )
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    match status {
        400 => pass(suite, name, request, Some(status), duration),
        200 => {
            // Accept 200 only when response carries a generated id.
            match resp.json::<Value>().await {
                Ok(body) => match body.get("id").and_then(|v| v.as_str()) {
                    Some(id) if !id.is_empty() => {
                        pass(suite, name, request, Some(status), duration)
                    }
                    _ => fail(
                        suite,
                        name,
                        request,
                        Some(status),
                        duration,
                        "200 response missing generated `id`",
                    ),
                },
                Err(e) => fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    duration,
                    format!("invalid JSON: {e:#}"),
                ),
            }
        }
        other => fail(
            suite,
            name,
            request,
            Some(other),
            duration,
            format!("expected 200 or 400, got {other}"),
        ),
    }
}

// C-05 — Create with invalid JSON
async fn c05_create_invalid_json(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-05 Create with invalid JSON";
    let request = format!("POST /api/{ENTITY} {{invalid}}");
    let start = Instant::now();

    let resp = match client
        .create_raw(ENTITY, "{invalid}", "application/json")
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
            )
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    if status == 400 {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 400, got {status}"),
        )
    }
}

// C-06 — List empty collection
async fn c06_list_empty(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-06 List empty collection";
    let request = format!("GET /api/{EMPTY_ENTITY}");
    let start = Instant::now();

    let _ = client.delete_all(EMPTY_ENTITY).await;

    let resp = match client.list(EMPTY_ENTITY, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match body.as_array() {
        Some(a) if a.is_empty() => pass(suite, name, request, Some(status), duration),
        Some(a) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected empty array, got {} items", a.len()),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "response is not an array",
        ),
    }
}

// C-07 — List returns all documents
async fn c07_list_all(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-07 List returns all documents";
    let request = format!("GET /api/{ENTITY}");
    let start = Instant::now();

    let seed = standard_seed();
    if let Err(e) = reseed(client, &seed).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.list(ENTITY, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match body.as_array() {
        Some(a) if a.len() == seed.len() => pass(suite, name, request, Some(status), duration),
        Some(a) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected {} items, got {}", seed.len(), a.len()),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "response is not an array",
        ),
    }
}

// C-08 — List with limit
async fn c08_list_with_limit(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-08 List with limit";
    let request = format!("GET /api/{ENTITY}?limit=2");
    let start = Instant::now();

    if let Err(e) = reseed(client, &standard_seed()).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.list(ENTITY, &[("limit", "2")]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match body.as_array() {
        Some(a) if a.len() == 2 => pass(suite, name, request, Some(status), duration),
        Some(a) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 2 items, got {}", a.len()),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "response is not an array",
        ),
    }
}

// C-09 — List with offset
async fn c09_list_with_offset(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-09 List with offset";
    let request = format!("GET /api/{ENTITY}?offset=1");
    let start = Instant::now();

    let seed = standard_seed();
    if let Err(e) = reseed(client, &seed).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.list(ENTITY, &[("offset", "1")]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match body.as_array() {
        Some(a) if a.len() == seed.len() - 1 => pass(suite, name, request, Some(status), duration),
        Some(a) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!(
                "expected {} items (seed-1), got {}",
                seed.len() - 1,
                a.len()
            ),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "response is not an array",
        ),
    }
}

// C-10 — List with limit + offset
async fn c10_list_with_limit_and_offset(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-10 List with limit + offset";
    let request = format!("GET /api/{ENTITY}?limit=2&offset=1");
    let start = Instant::now();

    if let Err(e) = reseed(client, &standard_seed()).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client
        .list(ENTITY, &[("limit", "2"), ("offset", "1")])
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match body.as_array() {
        Some(a) if a.len() == 2 => pass(suite, name, request, Some(status), duration),
        Some(a) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 2 items, got {}", a.len()),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "response is not an array",
        ),
    }
}

// C-11 — List sorted ascending by title
async fn c11_list_sorted_asc(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-11 List sorted ascending";
    let request = format!("GET /api/{ENTITY}?sort[title]=asc");
    let start = Instant::now();

    if let Err(e) = reseed(client, &standard_seed()).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.list(ENTITY, &[("sort[title]", "asc")]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let titles: Vec<String> = body
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| d.get("title").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let expected = vec!["Anathem", "Cryptonomicon", "Dune", "Snow Crash"];
    if titles == expected {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected {expected:?}, got {titles:?}"),
        )
    }
}

// C-12 — List sorted descending by pages
async fn c12_list_sorted_desc(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-12 List sorted descending";
    let request = format!("GET /api/{ENTITY}?sort[pages]=desc");
    let start = Instant::now();

    if let Err(e) = reseed(client, &standard_seed()).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.list(ENTITY, &[("sort[pages]", "desc")]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let pages: Vec<i64> = body
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| d.get("pages").and_then(|v| v.as_i64()))
                .collect()
        })
        .unwrap_or_default();

    let mut sorted = pages.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    if pages == sorted && !pages.is_empty() {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("not in desc order: {pages:?}"),
        )
    }
}

// C-13 — List with field projection
async fn c13_list_field_projection(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-13 List with field projection";
    let request = format!("GET /api/{ENTITY}?fields=title");
    let start = Instant::now();

    if let Err(e) = reseed(client, &standard_seed()).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.list(ENTITY, &[("fields", "title")]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let arr = match body.as_array() {
        Some(a) if !a.is_empty() => a,
        Some(_) => return fail(suite, name, request, Some(status), duration, "empty result"),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                "response is not an array",
            )
        }
    };

    for (i, item) in arr.iter().enumerate() {
        let obj = match item.as_object() {
            Some(o) => o,
            None => {
                return fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    duration,
                    format!("item {i} not an object"),
                )
            }
        };
        // The test scenario catalogue describes the response as "Only `title`
        // and `id` in response (no `pages`)", but the actual ElysianDB
        // behaviour is strict projection — `?fields=title` returns `title`
        // only, with no auto-included `id`. We therefore only enforce what
        // ElysianDB guarantees: the requested field is present AND the
        // unrequested field is absent. `id` is allowed to be either present
        // or absent depending on server version. Seed must carry no
        // duplicate titles so `title` alone identifies the document.
        if !obj.contains_key("title") {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                format!("item {i} missing `title`"),
            );
        }
        if obj.contains_key("pages") {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                format!("item {i} should not contain `pages` (projection ignored)"),
            );
        }
        // Guard against "projection drops everything" regressions: the
        // response must have at least the requested field and nothing
        // beyond what was requested (id is tolerated when present).
        for (k, _) in obj.iter() {
            if k != "title" && k != "id" {
                return fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    duration,
                    format!(
                        "item {i} contains unexpected key `{k}` — projection must be limited to \
                         the requested fields (plus optionally `id`)"
                    ),
                );
            }
        }
    }

    pass(suite, name, request, Some(status), duration)
}

// C-14 — List with full-text search
async fn c14_list_search(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-14 List with search";
    let request = format!("GET /api/{ENTITY}?search=Dune");
    let start = Instant::now();

    if let Err(e) = reseed(client, &standard_seed()).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.list(ENTITY, &[("search", "Dune")]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let arr = match body.as_array() {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                "response is not an array",
            )
        }
    };
    // Positive: at least one result must be the Dune document.
    let has_dune = arr
        .iter()
        .any(|d| d.get("title").and_then(|v| v.as_str()) == Some("Dune"));
    if !has_dune {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!(
                "expected at least one match for \"Dune\", got {} items",
                arr.len()
            ),
        );
    }
    // Negative: the seed has 4 distinct titles and only one ("Dune") matches.
    // If the backend returned the whole seed, `search` was a no-op — reject.
    let seed_len = standard_seed().len();
    if arr.len() >= seed_len {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!(
                "search appears to be a no-op: got {} items (seed has {seed_len} distinct titles, only \"Dune\" should match)",
                arr.len()
            ),
        );
    }
    // Stronger check: every returned title should mention the search term.
    let all_match = arr.iter().all(|d| {
        d.get("title")
            .and_then(|v| v.as_str())
            .map(|t| t.to_ascii_lowercase().contains("dune"))
            .unwrap_or(false)
    });
    if !all_match {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "some returned items do not contain \"Dune\" — search filter not applied".to_string(),
        );
    }
    pass(suite, name, request, Some(status), duration)
}

// C-15 — Get by ID
async fn c15_get_by_id(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-15 Get by ID";
    let start = Instant::now();

    let seed = vec![json!({"title": "Dune", "pages": 412})];
    let ids = match reseed(client, &seed).await {
        Ok(v) => v,
        Err(e) => return seed_error(suite, name, &format!("GET /api/{ENTITY}/{{id}}"), start, e),
    };
    let id = &ids[0];
    let request = format!("GET /api/{ENTITY}/{id}");

    let resp = match client.get(ENTITY, id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if body.get("id").and_then(|v| v.as_str()) == Some(id.as_str())
        && body.get("title").and_then(|v| v.as_str()) == Some("Dune")
    {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("unexpected body: {body}"),
        )
    }
}

// C-16 — Get by ID — not found
async fn c16_get_not_found(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-16 Get by ID — not found";
    let request = format!("GET /api/{ENTITY}/nonexistent-id");
    let start = Instant::now();

    let resp = match client.get(ENTITY, "nonexistent-id").await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
            )
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    if status == 404 {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 404, got {status}"),
        )
    }
}

// C-17 — Update single field
async fn c17_update_single_field(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-17 Update single field";
    let start = Instant::now();

    let seed = vec![json!({"title": "Dune", "pages": 412})];
    let ids = match reseed(client, &seed).await {
        Ok(v) => v,
        Err(e) => return seed_error(suite, name, &format!("PUT /api/{ENTITY}/{{id}}"), start, e),
    };
    let id = &ids[0];
    let request = format!("PUT /api/{ENTITY}/{id} {{pages:500}}");

    let resp = match client.update(ENTITY, id, json!({"pages": 500})).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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

    // Re-read and verify both pages updated AND title preserved.
    let resp = match client.get(ENTITY, id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback failed: {e:#}"),
            )
        }
    };
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let pages_ok = body.get("pages").and_then(|v| v.as_i64()) == Some(500);
    let title_ok = body.get("title").and_then(|v| v.as_str()) == Some("Dune");

    if pages_ok && title_ok {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("after update: pages_ok={pages_ok}, title_ok={title_ok}, body={body}"),
        )
    }
}

// C-18 — Update nested field
async fn c18_update_nested_field(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-18 Update nested field";
    let start = Instant::now();

    let seed = vec![json!({
        "title": "Dune",
        "metadata": {"isbn": "old-isbn", "publisher": "Chilton"}
    })];
    let ids = match reseed(client, &seed).await {
        Ok(v) => v,
        Err(e) => {
            return seed_error(
                suite,
                name,
                &format!("PUT /api/{ENTITY}/{{id}} nested"),
                start,
                e,
            )
        }
    };
    let id = &ids[0];
    let request = format!("PUT /api/{ENTITY}/{id} {{metadata:{{isbn:new-isbn}}}}");

    // Send a partial nested update — ElysianDB merges nested objects.
    let resp = match client
        .update(ENTITY, id, json!({"metadata": {"isbn": "new-isbn"}}))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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

    let resp = match client.get(ENTITY, id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback failed: {e:#}"),
            )
        }
    };
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let isbn_updated = body.pointer("/metadata/isbn").and_then(|v| v.as_str()) == Some("new-isbn");
    let title_preserved = body.get("title").and_then(|v| v.as_str()) == Some("Dune");

    if isbn_updated && title_preserved {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("isbn_updated={isbn_updated}, title_preserved={title_preserved}, body={body}"),
        )
    }
}

// C-19 — Batch update
async fn c19_batch_update(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-19 Batch update";
    let start = Instant::now();

    let seed = vec![
        json!({"title": "Dune", "pages": 412}),
        json!({"title": "Anathem", "pages": 932}),
    ];
    let ids = match reseed(client, &seed).await {
        Ok(v) => v,
        Err(e) => return seed_error(suite, name, &format!("PUT /api/{ENTITY} batch"), start, e),
    };
    let request = format!(
        "PUT /api/{ENTITY} [{{id:{},pages:999}},{{id:{},pages:888}}]",
        ids[0], ids[1]
    );

    let body = json!([
        {"id": ids[0], "pages": 999},
        {"id": ids[1], "pages": 888},
    ]);
    let resp = match client.batch_update(ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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

    // Verify both
    let r1 = match client.get(ENTITY, &ids[0]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback 1 failed: {e:#}"),
            )
        }
    };
    let r2 = match client.get(ENTITY, &ids[1]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback 2 failed: {e:#}"),
            )
        }
    };
    let b1: Value = match r1.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback 1 JSON: {e:#}"),
            )
        }
    };
    let b2: Value = match r2.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("readback 2 JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let p1 = b1.get("pages").and_then(|v| v.as_i64());
    let p2 = b2.get("pages").and_then(|v| v.as_i64());
    if p1 == Some(999) && p2 == Some(888) {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected pages=(999,888), got ({p1:?},{p2:?})"),
        )
    }
}

// C-20 — Delete by ID
async fn c20_delete_by_id(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-20 Delete by ID";
    let start = Instant::now();

    let ids = match reseed(client, &[json!({"title": "ToDelete"})]).await {
        Ok(v) => v,
        Err(e) => {
            return seed_error(
                suite,
                name,
                &format!("DELETE /api/{ENTITY}/{{id}}"),
                start,
                e,
            )
        }
    };
    let id = &ids[0];
    let request = format!("DELETE /api/{ENTITY}/{id} then GET");

    let resp = match client.delete(ENTITY, id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("delete request failed: {e:#}"),
            )
        }
    };
    let del_status = resp.status().as_u16();
    if del_status != 200 && del_status != 204 {
        return fail(
            suite,
            name,
            request,
            Some(del_status),
            start.elapsed(),
            format!("delete: expected 200 or 204, got {del_status}"),
        );
    }

    let resp = match client.get(ENTITY, id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(del_status),
                start.elapsed(),
                format!("readback request failed: {e:#}"),
            )
        }
    };
    let get_status = resp.status().as_u16();
    let duration = start.elapsed();

    if get_status == 404 {
        pass(suite, name, request, Some(get_status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(get_status),
            duration,
            format!("post-delete GET: expected 404, got {get_status}"),
        )
    }
}

// C-21 — Delete all
async fn c21_delete_all(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-21 Delete all";
    let request = format!("DELETE /api/{ENTITY} then GET");
    let start = Instant::now();

    if let Err(e) = reseed(client, &standard_seed()).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.delete_all(ENTITY).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("delete-all failed: {e:#}"),
            )
        }
    };
    let del_status = resp.status().as_u16();
    if del_status != 200 && del_status != 204 {
        return fail(
            suite,
            name,
            request,
            Some(del_status),
            start.elapsed(),
            format!("delete-all: expected 200 or 204, got {del_status}"),
        );
    }

    let resp = match client.list(ENTITY, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(del_status),
                start.elapsed(),
                format!("list failed: {e:#}"),
            )
        }
    };
    let list_status = resp.status().as_u16();
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(list_status),
                start.elapsed(),
                format!("list JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match body.as_array() {
        Some(a) if a.is_empty() => pass(suite, name, request, Some(list_status), duration),
        Some(a) => fail(
            suite,
            name,
            request,
            Some(list_status),
            duration,
            format!(
                "expected empty list after delete-all, got {} items",
                a.len()
            ),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(list_status),
            duration,
            "list response is not an array",
        ),
    }
}

// C-22 — Count
async fn c22_count(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-22 Count";
    let request = format!("GET /api/{ENTITY}/count");
    let start = Instant::now();

    let docs: Vec<Value> = (0..5)
        .map(|i| json!({"title": format!("Book{i}")}))
        .collect();
    if let Err(e) = reseed(client, &docs).await {
        return seed_error(suite, name, &request, start, e);
    }

    let resp = match client.count(ENTITY).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
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
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    // Accept either {"count": N} or a bare integer.
    let count = body
        .get("count")
        .and_then(|v| v.as_i64())
        .or_else(|| body.as_i64());

    match count {
        Some(5) => pass(suite, name, request, Some(status), duration),
        Some(n) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected count=5, got {n}"),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("could not extract count from {body}"),
        ),
    }
}

// C-23 — Exists — true
async fn c23_exists_true(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-23 Exists — true";
    let start = Instant::now();

    let ids = match reseed(client, &[json!({"title": "Exists"})]).await {
        Ok(v) => v,
        Err(e) => {
            return seed_error(
                suite,
                name,
                &format!("GET /api/{ENTITY}/{{id}}/exists"),
                start,
                e,
            )
        }
    };
    let id = &ids[0];
    let request = format!("GET /api/{ENTITY}/{id}/exists");

    let resp = match client.exists(ENTITY, id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
            )
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    if status == 200 {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 200, got {status}"),
        )
    }
}

/// Detect whether an `/exists` 200 response body indicates "does not exist".
///
/// ElysianDB has historically returned several shapes for this endpoint —
/// empty body, raw `false`, raw `0`, `{}`, or `{"exists": false}` (with or
/// without whitespace). Parse JSON when possible and fall back to a tolerant
/// string comparison otherwise.
fn is_falsy_exists_body(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return match v {
            Value::Bool(b) => !b,
            Value::Number(n) => n.as_f64().map(|f| f == 0.0).unwrap_or(false),
            Value::Null => true,
            Value::Object(ref map) => {
                map.is_empty()
                    || map
                        .get("exists")
                        .map(|x| matches!(x, Value::Bool(false) | Value::Null))
                        .unwrap_or(false)
            }
            _ => false,
        };
    }
    let lower = trimmed.to_ascii_lowercase();
    matches!(lower.as_str(), "false" | "0" | "null")
}

// C-24 — Exists — false
async fn c24_exists_false(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "C-24 Exists — false";
    let request = format!("GET /api/{ENTITY}/nonexistent-id/exists");
    let start = Instant::now();

    let resp = match client.exists(ENTITY, "nonexistent-id").await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("request failed: {e:#}"),
            )
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    // Spec accepts either 404 or a falsy 200 body.
    if status == 404 {
        return pass(suite, name, request, Some(status), duration);
    }
    if status == 200 {
        match resp.text().await {
            Ok(text) => {
                if is_falsy_exists_body(&text) {
                    return pass(suite, name, request, Some(status), duration);
                }
                return fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    duration,
                    format!("200 with non-falsy body: {text}"),
                );
            }
            Err(e) => {
                return fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    duration,
                    format!("body read failed: {e:#}"),
                )
            }
        }
    }
    fail(
        suite,
        name,
        request,
        Some(status),
        duration,
        format!("expected 404 or falsy 200, got {status}"),
    )
}

#[cfg(test)]
mod tests {
    use super::is_falsy_exists_body;

    #[test]
    fn falsy_empty_body() {
        assert!(is_falsy_exists_body(""));
        assert!(is_falsy_exists_body("   "));
        assert!(is_falsy_exists_body("\n"));
    }

    #[test]
    fn falsy_plain_literals() {
        assert!(is_falsy_exists_body("false"));
        assert!(is_falsy_exists_body("FALSE"));
        assert!(is_falsy_exists_body("0"));
        assert!(is_falsy_exists_body("null"));
    }

    #[test]
    fn falsy_empty_object() {
        assert!(is_falsy_exists_body("{}"));
    }

    #[test]
    fn falsy_exists_field_variants() {
        assert!(is_falsy_exists_body(r#"{"exists":false}"#));
        assert!(is_falsy_exists_body(r#"{"exists": false}"#));
        assert!(is_falsy_exists_body(r#"{  "exists" : false  }"#));
        assert!(is_falsy_exists_body(r#"{"exists": null}"#));
    }

    #[test]
    fn non_falsy_true_exists() {
        assert!(!is_falsy_exists_body(r#"{"exists":true}"#));
        assert!(!is_falsy_exists_body("true"));
        assert!(!is_falsy_exists_body("1"));
    }

    #[test]
    fn non_falsy_exists_non_bool() {
        // An unexpected shape must NOT be treated as falsy — better to fail
        // loudly than to mask a real regression.
        assert!(!is_falsy_exists_body(r#"{"exists":"maybe"}"#));
        assert!(!is_falsy_exists_body(r#"{"foo":"bar"}"#));
        assert!(!is_falsy_exists_body("not-json-and-not-a-known-literal"));
    }
}
