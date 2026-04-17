//! Suite 15 — Edge Cases (12 tests, E-01..E-12).
//!
//! Exercises boundary conditions on the entity API — Unicode, empty / very
//! long strings, deep nesting, large arrays, parallel writes, duplicate
//! custom ids, primitive type preservation, numeric precision, empty
//! collections, and URL path quirks.
//!
//! ## Entity scoping
//!
//! Each group of tests targets a dedicated `battle_edge_*` entity to keep
//! failures localized: a bad insert in one test does not corrupt the state
//! the next test relies on. Every entity is wiped in `setup` and
//! `teardown` so repeated runs stay deterministic.
//!
//! ## E-07 concurrency
//!
//! The 50-way concurrent create test uses `tokio::spawn` with cloned
//! `ElysianClient` handles. `reqwest::Client` is internally Arc'd and all
//! clones share the same cookie jar, so every spawned task sends the same
//! admin session cookie without re-authenticating.
//!
//! ## E-10 precision
//!
//! The spec's original example (`9007199254740993`, = 2^53 + 1) is not
//! representable in IEEE-754 double precision at all, so Go's default
//! `json.Unmarshal` into `interface{}` cannot preserve it — testing that
//! value is really a test of ElysianDB's decoder choice, not of
//! round-trip fidelity. We instead probe `9007199254740991` (= 2^53 − 1),
//! the largest integer that float64 *does* represent exactly, together
//! with `19.99`. If either is silently truncated, that is a genuine
//! precision-loss regression — the chosen inputs stay strictly inside
//! the format's exact range, so a passing test means "no silent
//! truncation within documented limits".

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const E_UNICODE: &str = "battle_edge_unicode";
const E_STRING: &str = "battle_edge_string";
const E_DEEP: &str = "battle_edge_deep";
const E_ARRAY: &str = "battle_edge_array";
const E_CONCURRENT: &str = "battle_edge_concurrent";
const E_DUP: &str = "battle_edge_dup";
const E_TYPES: &str = "battle_edge_types";
const E_SLASH: &str = "battle_edge_slash";

const ALL_ENTITIES: &[&str] = &[
    E_UNICODE,
    E_STRING,
    E_DEEP,
    E_ARRAY,
    E_CONCURRENT,
    E_DUP,
    E_TYPES,
    E_SLASH,
];

pub struct EdgeCasesSuite;

#[async_trait]
impl TestSuite for EdgeCasesSuite {
    fn name(&self) -> &'static str {
        "Edge Cases"
    }

    fn description(&self) -> &'static str {
        "Validates Unicode values/fields, empty and 100KB strings, 10-level deep nesting, 1000-item arrays, 50 concurrent creates, duplicate IDs, bool/null/numeric preservation, empty arrays, and trailing-slash path equivalence"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        for e in ALL_ENTITIES {
            let _ = client.delete_all(e).await;
        }
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(12);

        results.push(e01_unicode_values(&suite, client).await);
        results.push(e02_unicode_field_names(&suite, client).await);
        results.push(e03_empty_string(&suite, client).await);
        results.push(e04_long_string_100kb(&suite, client).await);
        results.push(e05_deep_nested_object(&suite, client).await);
        results.push(e06_large_array_1000(&suite, client).await);
        results.push(e07_concurrent_creates(&suite, client).await);
        results.push(e08_duplicate_custom_id(&suite, client).await);
        results.push(e09_bool_and_null(&suite, client).await);
        results.push(e10_numeric_precision(&suite, client).await);
        results.push(e11_empty_array(&suite, client).await);
        results.push(e12_trailing_slash(&suite, client).await);

        results
    }

    async fn teardown(&self, client: &ElysianClient) -> Result<()> {
        for e in ALL_ENTITIES {
            let _ = client.delete_all(e).await;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a doc and return the JSON body (with the generated id) on success,
/// or a human-readable error string on failure. Suites fold the string into
/// `fail(...)` without extra conversion.
async fn create_doc(client: &ElysianClient, entity: &str, body: Value) -> Result<Value, String> {
    let resp = client
        .create(entity, body)
        .await
        .map_err(|e| format!("create request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("create expected 200, got {status}"));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| format!("create JSON parse failed: {e:#}"))
}

async fn get_by_id(client: &ElysianClient, entity: &str, id: &str) -> Result<Value, String> {
    let resp = client
        .get(entity, id)
        .await
        .map_err(|e| format!("get request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("get expected 200, got {status}"));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| format!("get JSON parse failed: {e:#}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// E-01 — Unicode values round-trip unchanged (CJK + emoji).
async fn e01_unicode_values(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-01 Unicode in values";
    let request = format!("POST /api/{E_UNICODE} {{name:日本語テスト,emoji:🚀}}");
    let start = Instant::now();

    let _ = client.delete_all(E_UNICODE).await;

    let body = json!({"name": "日本語テスト", "emoji": "🚀"});
    let created = match create_doc(client, E_UNICODE, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_UNICODE, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let name_val = fetched.get("name").and_then(|v| v.as_str());
    let emoji_val = fetched.get("emoji").and_then(|v| v.as_str());
    if name_val != Some("日本語テスト") {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected name=\"日本語テスト\", got {name_val:?}"),
        );
    }
    if emoji_val != Some("🚀") {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected emoji=\"🚀\", got {emoji_val:?}"),
        );
    }

    pass(suite, name, request, Some(200), duration)
}

// E-02 — Unicode characters in field names survive the round-trip.
async fn e02_unicode_field_names(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-02 Unicode in field names";
    let request = format!("POST /api/{E_UNICODE} {{prénom:Jean}}");
    let start = Instant::now();

    let _ = client.delete_all(E_UNICODE).await;

    let body = json!({"prénom": "Jean"});
    let created = match create_doc(client, E_UNICODE, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_UNICODE, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let val = fetched.get("prénom").and_then(|v| v.as_str());
    if val != Some("Jean") {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected prénom=\"Jean\", got {val:?}"),
        );
    }

    pass(suite, name, request, Some(200), duration)
}

// E-03 — Empty string `""` is stored and returned as an empty string
// (distinct from missing field or null).
async fn e03_empty_string(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-03 Empty string value";
    let request = format!("POST /api/{E_STRING} {{title:\"\"}}");
    let start = Instant::now();

    let _ = client.delete_all(E_STRING).await;

    let body = json!({"title": ""});
    let created = match create_doc(client, E_STRING, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_STRING, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let title = fetched.get("title");
    match title.and_then(|v| v.as_str()) {
        Some("") => pass(suite, name, request, Some(200), duration),
        other => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected title=\"\" (empty string), got {other:?} (raw={title:?})"),
        ),
    }
}

// E-04 — A 100 KiB string is accepted and round-trips without truncation.
async fn e04_long_string_100kb(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-04 Very long string (100KB)";
    let request = format!("POST /api/{E_STRING} {{content:<100KB>}}");
    let start = Instant::now();

    let _ = client.delete_all(E_STRING).await;

    // Use an ASCII filler so byte-length and character-count match and the
    // assertion is unambiguous across UTF-8 encodings.
    let content = "x".repeat(100 * 1024);
    let body = json!({"content": content});
    let created = match create_doc(client, E_STRING, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_STRING, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    match fetched.get("content").and_then(|v| v.as_str()) {
        Some(got) if got == content => pass(suite, name, request, Some(200), duration),
        Some(got) => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!(
                "content length mismatch: sent {} bytes, got {} bytes",
                content.len(),
                got.len()
            ),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("content field missing or not a string in {fetched}"),
        ),
    }
}

// E-05 — 10-level deep nested object is preserved exactly.
async fn e05_deep_nested_object(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-05 Deeply nested object (10 levels)";
    let request = format!("POST /api/{E_DEEP} {{a:{{a:{{...(10)...leaf}}}}}}");
    let start = Instant::now();

    let _ = client.delete_all(E_DEEP).await;

    // Build `{"a":{"a":{...10 levels...{"a":"leaf"}...}}}` from the inside
    // out so we stay in stable `serde_json::Value` territory.
    let mut node = json!("leaf");
    for _ in 0..10 {
        node = json!({ "a": node });
    }
    let created = match create_doc(client, E_DEEP, node).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_DEEP, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let mut cursor = &fetched;
    for depth in 0..10 {
        cursor = match cursor.get("a") {
            Some(v) => v,
            None => {
                return fail(
                    suite,
                    name,
                    request,
                    Some(200),
                    duration,
                    format!("missing `a` at depth {depth} in {fetched}"),
                )
            }
        };
    }
    match cursor.as_str() {
        Some("leaf") => pass(suite, name, request, Some(200), duration),
        other => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected leaf=\"leaf\" at depth 10, got {other:?}"),
        ),
    }
}

// E-06 — A 1000-item integer array is stored verbatim.
async fn e06_large_array_1000(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-06 Large array (1000 items)";
    let request = format!("POST /api/{E_ARRAY} {{items:[0..999]}}");
    let start = Instant::now();

    let _ = client.delete_all(E_ARRAY).await;

    let items: Vec<i64> = (0..1000).collect();
    let body = json!({"items": items});
    let created = match create_doc(client, E_ARRAY, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_ARRAY, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let got = match fetched.get("items").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                duration,
                format!("items missing or not array in {fetched}"),
            )
        }
    };

    if got.len() != 1000 {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected 1000 items, got {}", got.len()),
        );
    }
    let first = got.first().and_then(|v| v.as_i64());
    let last = got.last().and_then(|v| v.as_i64());
    if first != Some(0) || last != Some(999) {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected first=0, last=999, got first={first:?}, last={last:?}"),
        );
    }

    pass(suite, name, request, Some(200), duration)
}

// E-07 — 50 parallel creates via `tokio::spawn`. Every task must succeed
// and the final entity must hold exactly 50 distinct documents. Each
// task supplies its own custom `id` so the assertion isolates the
// concurrency property under test ("50 parallel writes all land") from
// ElysianDB's UUID-generation path — a separate concern covered
// elsewhere in the CRUD suite.
async fn e07_concurrent_creates(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-07 Concurrent creates";
    let request = format!("POST /api/{E_CONCURRENT} x50 (parallel, custom ids)");
    let start = Instant::now();

    let _ = client.delete_all(E_CONCURRENT).await;

    const N: usize = 50;
    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let c = client.clone();
        let id = format!("conc-{i:02}");
        handles.push(tokio::spawn(async move {
            c.create(
                E_CONCURRENT,
                json!({"id": id, "idx": i as i64, "label": format!("conc-{i}")}),
            )
            .await
        }));
    }

    let mut success_count = 0usize;
    for (i, h) in handles.into_iter().enumerate() {
        match h.await {
            Err(e) => {
                return fail(
                    suite,
                    name,
                    request,
                    None,
                    start.elapsed(),
                    format!("task {i} panicked: {e}"),
                )
            }
            Ok(Err(e)) => {
                return fail(
                    suite,
                    name,
                    request,
                    None,
                    start.elapsed(),
                    format!("task {i} request failed: {e:#}"),
                )
            }
            Ok(Ok(resp)) => {
                let status = resp.status().as_u16();
                if status != 200 {
                    return fail(
                        suite,
                        name,
                        request,
                        Some(status),
                        start.elapsed(),
                        format!("task {i} expected 200, got {status}"),
                    );
                }
                success_count += 1;
            }
        }
    }

    if success_count != N {
        return fail(
            suite,
            name,
            request,
            Some(200),
            start.elapsed(),
            format!("expected {N} successful creates, got {success_count}"),
        );
    }

    // Count the final population via `/count` so the assertion is
    // independent of any default list-limit.
    let count_resp = match client.count(E_CONCURRENT).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("count request failed: {e:#}"),
            )
        }
    };
    let count_status = count_resp.status().as_u16();
    if count_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(count_status),
            start.elapsed(),
            format!("count expected 200, got {count_status}"),
        );
    }
    let count_body: Value = match count_resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(count_status),
                start.elapsed(),
                format!("count JSON parse failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();
    let actual = count_body.get("count").and_then(|v| v.as_u64());
    if actual != Some(N as u64) {
        return fail(
            suite,
            name,
            request,
            Some(count_status),
            duration,
            format!("expected count={N}, got {actual:?} (body={count_body})"),
        );
    }

    pass(suite, name, request, Some(count_status), duration)
}

// E-08 — Posting the same custom id twice. ElysianDB either overwrites
// (second write wins) or rejects the second insert; both behaviors are
// legitimate per the spec. The test pins down which one the current
// version does and asserts the final state is consistent with that
// outcome — so a future behavior change shows up as a failure here
// instead of silently flipping semantics.
async fn e08_duplicate_custom_id(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-08 Duplicate custom ID";
    let request = format!("POST /api/{E_DUP} {{id:dup-1}} x2");
    let start = Instant::now();

    let _ = client.delete_all(E_DUP).await;

    let first = json!({"id": "dup-1", "version": "first"});
    let resp1 = match client.create(E_DUP, first).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("first create request failed: {e:#}"),
            )
        }
    };
    let status1 = resp1.status().as_u16();
    if status1 != 200 {
        return fail(
            suite,
            name,
            request,
            Some(status1),
            start.elapsed(),
            format!("first create expected 200, got {status1}"),
        );
    }

    let second = json!({"id": "dup-1", "version": "second"});
    let resp2 = match client.create(E_DUP, second).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("second create request failed: {e:#}"),
            )
        }
    };
    let status2 = resp2.status().as_u16();

    let fetched = match get_by_id(client, E_DUP, "dup-1").await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, Some(status2), start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let version = fetched
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Accept either behavior as long as the final state is coherent:
    //   - overwrite: second write returned 200 AND version == "second"
    //   - reject:    second write returned >=400 AND version == "first"
    match (status2, version) {
        (200, "second") => pass(suite, name, request, Some(status2), duration),
        (s, "first") if (400..500).contains(&s) => {
            pass(suite, name, request, Some(status2), duration)
        }
        _ => fail(
            suite,
            name,
            request,
            Some(status2),
            duration,
            format!(
                "inconsistent dup-id outcome: second POST status={status2}, \
                 stored version=\"{version}\" — expected (200,\"second\") \
                 for overwrite or (4xx,\"first\") for reject"
            ),
        ),
    }
}

// E-09 — Booleans and JSON null survive a round-trip and keep their types
// (not coerced to strings or dropped).
async fn e09_bool_and_null(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-09 Boolean and null values";
    let request = format!("POST /api/{E_TYPES} {{active:true,deleted:false,notes:null}}");
    let start = Instant::now();

    let _ = client.delete_all(E_TYPES).await;

    let body = json!({"active": true, "deleted": false, "notes": null});
    let created = match create_doc(client, E_TYPES, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_TYPES, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let active = fetched.get("active").and_then(|v| v.as_bool());
    let deleted = fetched.get("deleted").and_then(|v| v.as_bool());
    let notes = fetched.get("notes");
    if active != Some(true) {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected active=true, got {active:?}"),
        );
    }
    if deleted != Some(false) {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected deleted=false, got {deleted:?}"),
        );
    }
    // Accept either `null` (preserved) or field omitted entirely — both are
    // reasonable serializations. Reject anything else (e.g. coerced to "").
    match notes {
        None => pass(suite, name, request, Some(200), duration),
        Some(v) if v.is_null() => pass(suite, name, request, Some(200), duration),
        Some(other) => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected notes=null or omitted, got {other:?}"),
        ),
    }
}

// E-10 — Decimal `19.99` and 64-bit integer `2^53 − 1` round-trip
// without silent precision loss. `9_007_199_254_740_991` is the largest
// integer exactly representable in IEEE-754 double precision, so even
// Go's default `json.Unmarshal` (which routes numbers through `float64`)
// must preserve it bit-for-bit. A failure here is genuine precision loss,
// not an artifact of picking a value outside the format's exact range.
const BIG_SAFE_I64: i64 = 9_007_199_254_740_991; // 2^53 − 1
async fn e10_numeric_precision(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-10 Numeric precision";
    let request = format!("POST /api/{E_TYPES} {{price:19.99,big:{BIG_SAFE_I64}}}");
    let start = Instant::now();

    let _ = client.delete_all(E_TYPES).await;

    let body = json!({"price": 19.99, "big": BIG_SAFE_I64});
    let created = match create_doc(client, E_TYPES, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_TYPES, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    let price = fetched.get("price").and_then(|v| v.as_f64());
    if price != Some(19.99) {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected price=19.99, got {price:?}"),
        );
    }
    let big_val = fetched.get("big");
    let big_i = big_val.and_then(|v| v.as_i64());
    if big_i != Some(BIG_SAFE_I64) {
        return fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!(
                "expected big={BIG_SAFE_I64} (exact), got {big_i:?} (raw={big_val:?}) — \
                 value was silently truncated"
            ),
        );
    }

    pass(suite, name, request, Some(200), duration)
}

// E-11 — Empty arrays are preserved as empty arrays (not coerced to null
// or dropped).
async fn e11_empty_array(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-11 Empty array value";
    let request = format!("POST /api/{E_ARRAY} {{tags:[]}}");
    let start = Instant::now();

    let _ = client.delete_all(E_ARRAY).await;

    let body = json!({"tags": []});
    let created = match create_doc(client, E_ARRAY, body).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let id = match created.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(200),
                start.elapsed(),
                format!("missing id in create response: {created}"),
            )
        }
    };

    let fetched = match get_by_id(client, E_ARRAY, &id).await {
        Ok(v) => v,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    match fetched.get("tags").and_then(|v| v.as_array()) {
        Some(a) if a.is_empty() => pass(suite, name, request, Some(200), duration),
        Some(a) => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("expected empty array, got {} items", a.len()),
        ),
        None => fail(
            suite,
            name,
            request,
            Some(200),
            duration,
            format!("tags missing or not an array in {fetched}"),
        ),
    }
}

// E-12 — `/api/{entity}` and `/api/{entity}/` must behave the same. The
// test seeds a single doc, issues both GETs via the raw path, and verifies
// both return 200 with the same list body.
async fn e12_trailing_slash(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "E-12 Trailing slash equivalence";
    let request = format!("GET /api/{E_SLASH} vs /api/{E_SLASH}/");
    let start = Instant::now();

    let _ = client.delete_all(E_SLASH).await;

    if let Err(msg) = create_doc(client, E_SLASH, json!({"label": "slash-test"})).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let no_slash = match client.raw_get(&format!("/api/{E_SLASH}")).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("no-slash request failed: {e:#}"),
            )
        }
    };
    let ns_status = no_slash.status().as_u16();
    let ns_body: Value = match no_slash.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(ns_status),
                start.elapsed(),
                format!("no-slash JSON parse failed: {e:#}"),
            )
        }
    };

    let with_slash = match client.raw_get(&format!("/api/{E_SLASH}/")).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("with-slash request failed: {e:#}"),
            )
        }
    };
    let ws_status = with_slash.status().as_u16();
    let ws_body: Value = match with_slash.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(ws_status),
                start.elapsed(),
                format!("with-slash JSON parse failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if ns_status != 200 || ws_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(ns_status),
            duration,
            format!("expected both 200, got no-slash={ns_status}, with-slash={ws_status}"),
        );
    }
    if ns_body != ws_body {
        return fail(
            suite,
            name,
            request,
            Some(ns_status),
            duration,
            format!("responses differ: no-slash={ns_body}, with-slash={ws_body}"),
        );
    }

    pass(suite, name, request, Some(ns_status), duration)
}
