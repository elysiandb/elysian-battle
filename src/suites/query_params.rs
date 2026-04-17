//! Suite 4 — URL Query Parameters (8 tests, QP-01..QP-08).
//!
//! Exercises the URL-parameter form of the Query API: `filter[field][op]=`,
//! `sort[field]=asc|desc`, `fields=`, `search=`, `countOnly=true`, and
//! combinations thereof, all against `GET /api/battle_articles`.
//!
//! Reuses the same 20-document seed as Suite 3 (Query API). The suite
//! reseeds in its own `setup()` since the runner wipes `battle_articles`
//! between suites.

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::client::ElysianClient;
use crate::suites::query;
use crate::suites::{TestResult, TestStatus, TestSuite};

const ENTITY: &str = "battle_articles";

pub struct QueryParamsSuite;

#[async_trait]
impl TestSuite for QueryParamsSuite {
    fn name(&self) -> &'static str {
        "URL Query Parameters"
    }

    fn description(&self) -> &'static str {
        "Validates GET /api/{entity} URL parameters: filter[field][op], sort, fields, search, countOnly, and combinations"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        // Reuse the Query suite seed — 20 battle_articles.
        query::reseed(client).await
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(8);

        results.push(qp01_filter_eq(&suite, client).await);
        results.push(qp02_filter_gt(&suite, client).await);
        results.push(qp03_sort_asc(&suite, client).await);
        results.push(qp04_sort_desc(&suite, client).await);
        results.push(qp05_fields_projection(&suite, client).await);
        results.push(qp06_search(&suite, client).await);
        results.push(qp07_count_only(&suite, client).await);
        results.push(qp08_combined(&suite, client).await);

        results
    }

    async fn teardown(&self, client: &ElysianClient) -> Result<()> {
        let _ = client.delete_all(ENTITY).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Result helpers
// ---------------------------------------------------------------------------

fn pass(
    suite: &str,
    name: &str,
    request: String,
    status: Option<u16>,
    duration: Duration,
) -> TestResult {
    TestResult {
        suite: suite.to_string(),
        name: name.to_string(),
        status: TestStatus::Passed,
        duration,
        error: None,
        request: Some(request),
        response_status: status,
    }
}

fn fail(
    suite: &str,
    name: &str,
    request: String,
    status: Option<u16>,
    duration: Duration,
    error: impl Into<String>,
) -> TestResult {
    TestResult {
        suite: suite.to_string(),
        name: name.to_string(),
        status: TestStatus::Failed,
        duration,
        error: Some(error.into()),
        request: Some(request),
        response_status: status,
    }
}

// ---------------------------------------------------------------------------
// Shared list-query helper
// ---------------------------------------------------------------------------

/// Run `GET /api/{ENTITY}?<params>`, assert `200`, parse the body as an
/// array, then enforce `arr.len() == expected_count` and `predicate`
/// for every item.
#[allow(clippy::too_many_arguments)]
async fn run_list_filter_test(
    suite: &str,
    name: &str,
    request: String,
    client: &ElysianClient,
    params: &[(&str, &str)],
    expected_count: usize,
    predicate: impl Fn(&Value) -> bool,
    predicate_desc: &str,
) -> TestResult {
    let start = Instant::now();

    let resp = match client.list(ENTITY, params).await {
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
    let v: Value = match resp.json().await {
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

    let arr = match v.as_array() {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                format!("response is not an array: {v}"),
            )
        }
    };

    if arr.len() != expected_count {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected {expected_count} items, got {}", arr.len()),
        );
    }

    for (i, doc) in arr.iter().enumerate() {
        if !predicate(doc) {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                format!("doc {i} violates `{predicate_desc}`: {doc}"),
            );
        }
    }

    pass(suite, name, request, Some(status), duration)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// QP-01 — filter[status][eq]=published
async fn qp01_filter_eq(suite: &str, client: &ElysianClient) -> TestResult {
    run_list_filter_test(
        suite,
        "QP-01 filter[field][eq]",
        format!("GET /api/{ENTITY}?filter[status][eq]=published"),
        client,
        &[("filter[status][eq]", "published")],
        13,
        |doc| doc.get("status").and_then(|v| v.as_str()) == Some("published"),
        "status == \"published\"",
    )
    .await
}

// QP-02 — filter[views][gt]=500
async fn qp02_filter_gt(suite: &str, client: &ElysianClient) -> TestResult {
    run_list_filter_test(
        suite,
        "QP-02 filter[field][gt]",
        format!("GET /api/{ENTITY}?filter[views][gt]=500"),
        client,
        &[("filter[views][gt]", "500")],
        5,
        |doc| {
            doc.get("views")
                .and_then(|v| v.as_i64())
                .map(|n| n > 500)
                .unwrap_or(false)
        },
        "views > 500",
    )
    .await
}

// QP-03 — sort[views]=asc
async fn qp03_sort_asc(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "QP-03 sort[field]=asc";
    let request = format!("GET /api/{ENTITY}?sort[views]=asc");
    let start = Instant::now();

    let resp = match client.list(ENTITY, &[("sort[views]", "asc")]).await {
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

    let views: Vec<i64> = body
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| d.get("views").and_then(|v| v.as_i64()))
                .collect()
        })
        .unwrap_or_default();

    if views.len() != 20 {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 20 items, got {}", views.len()),
        );
    }

    let mut expected = views.clone();
    expected.sort();
    if views != expected {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("views not in asc order: {views:?}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// QP-04 — sort[title]=desc
async fn qp04_sort_desc(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "QP-04 sort[field]=desc";
    let request = format!("GET /api/{ENTITY}?sort[title]=desc");
    let start = Instant::now();

    let resp = match client.list(ENTITY, &[("sort[title]", "desc")]).await {
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

    if titles.len() != 20 {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 20 items, got {}", titles.len()),
        );
    }

    let mut expected = titles.clone();
    expected.sort_by(|a, b| b.cmp(a));
    if titles != expected {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("titles not in desc order: {titles:?}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// QP-05 — fields=title,status
//
// Projection is strict in ElysianDB: `id` is not auto-included. Every
// returned item must carry `title` and `status` and MUST NOT carry any of
// the seeded extra fields (`views`, `tags`, `category`, `metadata`). We
// tolerate an `id` key since some builds return it, but reject every
// other unexpected key.
async fn qp05_fields_projection(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "QP-05 fields projection";
    let request = format!("GET /api/{ENTITY}?fields=title,status");
    let start = Instant::now();

    let resp = match client.list(ENTITY, &[("fields", "title,status")]).await {
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
                    format!("item {i} is not an object"),
                )
            }
        };
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
        if !obj.contains_key("status") {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                format!("item {i} missing `status`"),
            );
        }
        for k in obj.keys() {
            if k != "title" && k != "status" && k != "id" {
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

// QP-06 — search=Rust
//
// Assertion strategy mirrors Suite 2's C-14: at least one document with
// "Rust" in the title, fewer results than the full seed (so the search
// filter is actually applied), and every returned title mentions "rust"
// (case-insensitive).
async fn qp06_search(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "QP-06 search";
    let request = format!("GET /api/{ENTITY}?search=Rust");
    let start = Instant::now();

    let resp = match client.list(ENTITY, &[("search", "Rust")]).await {
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

    if arr.is_empty() {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "expected at least one match for \"Rust\", got 0 items",
        );
    }

    if arr.len() >= 20 {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!(
                "search appears to be a no-op: got {} items (seed has 20 docs, only 4 titles match \"Rust\")",
                arr.len()
            ),
        );
    }

    let all_match = arr.iter().all(|d| {
        d.get("title")
            .and_then(|v| v.as_str())
            .map(|t| t.to_ascii_lowercase().contains("rust"))
            .unwrap_or(false)
    });
    if !all_match {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            "some returned items do not contain \"Rust\" in their title — search filter not applied",
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// QP-07 — countOnly=true
async fn qp07_count_only(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "QP-07 countOnly";
    let request = format!("GET /api/{ENTITY}?countOnly=true");
    let start = Instant::now();

    let resp = match client.list(ENTITY, &[("countOnly", "true")]).await {
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
    let v: Value = match resp.json().await {
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

    // Accept `{"count": 20}` or bare integer `20`.
    let count = v
        .get("count")
        .and_then(|x| x.as_i64())
        .or_else(|| v.as_i64());

    match count {
        Some(20) => pass(suite, name, request, Some(status), duration),
        Some(n) => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected count=20, got {n}"),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("could not extract count from {v}"),
        ),
    }
}

// QP-08 — Combined: filter + sort + limit + fields
//
// Asserts the composition: published articles sorted by views desc, limited
// to 3, projected to `title` only. Expected top-3 titles (views 1000, 950,
// 750) are "Music Trends", "Intro to Rust", "API Design".
async fn qp08_combined(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "QP-08 Combined params";
    let request = format!(
        "GET /api/{ENTITY}?filter[status][eq]=published&sort[views]=desc&limit=3&fields=title"
    );
    let start = Instant::now();

    let params = &[
        ("filter[status][eq]", "published"),
        ("sort[views]", "desc"),
        ("limit", "3"),
        ("fields", "title"),
    ];
    let resp = match client.list(ENTITY, params).await {
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
            format!("expected 3 items (limit=3), got {}", arr.len()),
        );
    }

    // Projection: each item must expose `title` and must NOT expose any
    // non-requested seed field (`id` is tolerated, see QP-05).
    let forbidden = ["status", "views", "tags", "category", "metadata"];
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
        for f in &forbidden {
            if obj.contains_key(*f) {
                return fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    duration,
                    format!(
                        "item {i} contains forbidden key `{f}` — projection did not restrict fields"
                    ),
                );
            }
        }
    }

    let titles: Vec<&str> = arr
        .iter()
        .filter_map(|d| d.get("title").and_then(|v| v.as_str()))
        .collect();
    let expected = vec!["Music Trends", "Intro to Rust", "API Design"];
    if titles != expected {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected titles {expected:?}, got {titles:?}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}
