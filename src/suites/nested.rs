//! Suite 5 — Nested Entities (6 tests, N-01..N-06).
//!
//! Exercises ElysianDB's `@entity` sub-entity creation mechanism (see
//! `internal/api/relationship.go:ExtractSubEntities`):
//!
//!   - N-01 Single-level `@entity` creates a separate sub-entity and replaces
//!     the parent's field with `{"@entity":"<name>","id":"<uuid>"}`.
//!   - N-02 Three-level nesting creates an entity at every level.
//!   - N-03 An `@entity` reference whose `id` is the only non-`@entity`
//!     field links to the existing sub-entity (no duplicate created).
//!   - N-04 `?includes=<field>` expands a sub-entity reference into the full
//!     stored document. The include parameter is a FIELD name, not an
//!     entity name (see `internal/api/include.go:applyIncludesRecursive`).
//!   - N-05 `?includes=all` recursively expands every `@entity` reference.
//!   - N-06 Arrays of nested `@entity` objects each become separate documents.
//!
//! Each test creates its own posts/authors/jobs/comments and asserts via
//! cross-entity reads that the sub-documents actually exist with the expected
//! fields.

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const POSTS: &str = "battle_posts";
const AUTHORS: &str = "battle_authors_nested";
const JOBS: &str = "battle_jobs_nested";
const COMMENTS: &str = "battle_comments";

pub struct NestedSuite;

#[async_trait]
impl TestSuite for NestedSuite {
    fn name(&self) -> &'static str {
        "Nested Entities"
    }

    fn description(&self) -> &'static str {
        "Validates @entity sub-entity creation, deep nesting, existing-id linking, includes expansion, and arrays of nested entities"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        // Each test seeds its own data; just make sure no leftovers remain.
        for entity in [POSTS, AUTHORS, JOBS, COMMENTS] {
            let _ = client.delete_all(entity).await;
        }
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(6);

        results.push(n01_create_with_at_entity(&suite, client).await);
        results.push(n02_deep_nesting_three_levels(&suite, client).await);
        results.push(n03_at_entity_with_existing_id(&suite, client).await);
        results.push(n04_includes_expands_nested(&suite, client).await);
        results.push(n05_includes_all(&suite, client).await);
        results.push(n06_array_of_nested_entities(&suite, client).await);

        results
    }

    async fn teardown(&self, client: &ElysianClient) -> Result<()> {
        for entity in [POSTS, AUTHORS, JOBS, COMMENTS] {
            let _ = client.delete_all(entity).await;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Wipe every entity used by this suite so a single test starts from a known
/// empty state regardless of execution order.
async fn wipe_all(client: &ElysianClient) {
    for entity in [POSTS, AUTHORS, JOBS, COMMENTS] {
        let _ = client.delete_all(entity).await;
    }
}

/// Extract a `{"@entity":"<expected>","id":"<uuid>"}` reference from `parent`
/// at `field`. Returns the referenced ID on success, or a descriptive error
/// when the shape is wrong — used to keep the test bodies short.
fn ref_id<'a>(parent: &'a Value, field: &str, expected_entity: &str) -> Result<&'a str, String> {
    let r = parent
        .get(field)
        .ok_or_else(|| format!("missing field `{field}` in parent: {parent}"))?;
    let actual = r.get("@entity").and_then(|v| v.as_str()).unwrap_or("");
    if actual != expected_entity {
        return Err(format!(
            "field `{field}` @entity mismatch: expected `{expected_entity}`, got `{actual}`"
        ));
    }
    let id = r
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("field `{field}` missing id: {r}"))?;
    if id.is_empty() {
        return Err(format!("field `{field}` has empty id"));
    }
    Ok(id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// N-01 — Create with @entity
//
// POST /api/battle_posts {"title":"Post","author":{"@entity":"battle_authors_nested","fullname":"Alice"}}
// Verifies:
//   1. The post is stored with `author = {"@entity":"battle_authors_nested","id":"<uuid>"}`.
//   2. GET /api/battle_authors_nested/<uuid> returns the author with `fullname:"Alice"`.
async fn n01_create_with_at_entity(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "N-01 Create with @entity";
    let request = format!("POST /api/{POSTS} {{title,author:{{@entity:{AUTHORS}}}}}");
    let start = Instant::now();

    wipe_all(client).await;

    let body = json!({
        "title": "Post",
        "author": {
            "@entity": AUTHORS,
            "fullname": "Alice"
        }
    });
    let resp = match client.create(POSTS, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("create failed: {e:#}"),
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
    let post: Value = match resp.json().await {
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

    // Verify the author reference shape on the returned post.
    let author_id = match ref_id(&post, "author", AUTHORS) {
        Ok(id) => id.to_string(),
        Err(e) => return fail(suite, name, request, Some(status), start.elapsed(), e),
    };

    // Cross-entity verification: the author was actually created.
    let resp = match client.get(AUTHORS, &author_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("author readback failed: {e:#}"),
            )
        }
    };
    let author_status = resp.status().as_u16();
    if author_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(author_status),
            start.elapsed(),
            format!("author GET expected 200, got {author_status}"),
        );
    }
    let author: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(author_status),
                start.elapsed(),
                format!("author JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if author.get("fullname").and_then(|v| v.as_str()) != Some("Alice") {
        return fail(
            suite,
            name,
            request,
            Some(author_status),
            duration,
            format!("author fullname mismatch: {author}"),
        );
    }
    if author.get("id").and_then(|v| v.as_str()) != Some(author_id.as_str()) {
        return fail(
            suite,
            name,
            request,
            Some(author_status),
            duration,
            format!("author id mismatch: expected {author_id}, body {author}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// N-02 — Deep nesting (3 levels): Post → Author → Job
async fn n02_deep_nesting_three_levels(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "N-02 Deep nesting (3 levels)";
    let request = format!("POST /api/{POSTS} (post→author→job)");
    let start = Instant::now();

    wipe_all(client).await;

    let body = json!({
        "title": "Deep",
        "author": {
            "@entity": AUTHORS,
            "fullname": "Bob",
            "job": {
                "@entity": JOBS,
                "title": "Engineer"
            }
        }
    });
    let resp = match client.create(POSTS, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("create failed: {e:#}"),
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
    let post: Value = match resp.json().await {
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

    let author_id = match ref_id(&post, "author", AUTHORS) {
        Ok(id) => id.to_string(),
        Err(e) => return fail(suite, name, request, Some(status), start.elapsed(), e),
    };

    // Read the author and verify the nested job reference.
    let resp = match client.get(AUTHORS, &author_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("author readback failed: {e:#}"),
            )
        }
    };
    let author_status = resp.status().as_u16();
    if author_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(author_status),
            start.elapsed(),
            format!("author GET expected 200, got {author_status}"),
        );
    }
    let author: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(author_status),
                start.elapsed(),
                format!("author JSON: {e:#}"),
            )
        }
    };
    if author.get("fullname").and_then(|v| v.as_str()) != Some("Bob") {
        return fail(
            suite,
            name,
            request,
            Some(author_status),
            start.elapsed(),
            format!("author fullname mismatch: {author}"),
        );
    }

    let job_id = match ref_id(&author, "job", JOBS) {
        Ok(id) => id.to_string(),
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(author_status),
                start.elapsed(),
                e,
            )
        }
    };

    // Verify the job exists with the expected fields.
    let resp = match client.get(JOBS, &job_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(author_status),
                start.elapsed(),
                format!("job readback failed: {e:#}"),
            )
        }
    };
    let job_status = resp.status().as_u16();
    if job_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(job_status),
            start.elapsed(),
            format!("job GET expected 200, got {job_status}"),
        );
    }
    let job: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(job_status),
                start.elapsed(),
                format!("job JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if job.get("title").and_then(|v| v.as_str()) != Some("Engineer") {
        return fail(
            suite,
            name,
            request,
            Some(job_status),
            duration,
            format!("job title mismatch: {job}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// N-03 — @entity with existing ID links to the existing sub-entity
//
// `internal/api/relationship.go:handleMapSubEntity` skips creation when the
// embedded `@entity` object has only `@entity` + `id` and no other fields:
// it just rewrites the parent reference and returns. This test creates an
// author first, then a post that references that author by id only, and
// verifies (a) the post links to that exact id, and (b) only one author
// document exists in `battle_authors_nested`.
async fn n03_at_entity_with_existing_id(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "N-03 @entity with existing ID";
    let request = format!("POST /api/{POSTS} {{author:{{@entity:{AUTHORS},id:<existing>}}}}");
    let start = Instant::now();

    wipe_all(client).await;

    // Step 1: create the author up front.
    let resp = match client.create(AUTHORS, json!({"fullname": "Carol"})).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("author create failed: {e:#}"),
            )
        }
    };
    let astatus = resp.status().as_u16();
    if astatus != 200 {
        return fail(
            suite,
            name,
            request,
            Some(astatus),
            start.elapsed(),
            format!("author create expected 200, got {astatus}"),
        );
    }
    let author: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(astatus),
                start.elapsed(),
                format!("author JSON: {e:#}"),
            )
        }
    };
    let existing_id = match author.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return fail(
                suite,
                name,
                request,
                Some(astatus),
                start.elapsed(),
                "author response missing id",
            )
        }
    };

    // Step 2: post referencing the existing author by id only.
    let body = json!({
        "title": "Linked",
        "author": {"@entity": AUTHORS, "id": existing_id.clone()}
    });
    let resp = match client.create(POSTS, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("post create failed: {e:#}"),
            )
        }
    };
    let pstatus = resp.status().as_u16();
    if pstatus != 200 {
        return fail(
            suite,
            name,
            request,
            Some(pstatus),
            start.elapsed(),
            format!("post create expected 200, got {pstatus}"),
        );
    }
    let post: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(pstatus),
                start.elapsed(),
                format!("post JSON: {e:#}"),
            )
        }
    };

    let linked_id = match ref_id(&post, "author", AUTHORS) {
        Ok(id) => id.to_string(),
        Err(e) => return fail(suite, name, request, Some(pstatus), start.elapsed(), e),
    };
    if linked_id != existing_id {
        return fail(
            suite,
            name,
            request,
            Some(pstatus),
            start.elapsed(),
            format!("expected post.author.id={existing_id}, got {linked_id}"),
        );
    }

    // Step 3: confirm no duplicate author was created.
    let resp = match client.list(AUTHORS, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(pstatus),
                start.elapsed(),
                format!("authors list failed: {e:#}"),
            )
        }
    };
    let lstatus = resp.status().as_u16();
    if lstatus != 200 {
        return fail(
            suite,
            name,
            request,
            Some(lstatus),
            start.elapsed(),
            format!("authors list expected 200, got {lstatus}"),
        );
    }
    let list: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(lstatus),
                start.elapsed(),
                format!("authors list JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let arr = match list.as_array() {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(lstatus),
                duration,
                "authors list is not an array",
            )
        }
    };
    if arr.len() != 1 {
        return fail(
            suite,
            name,
            request,
            Some(lstatus),
            duration,
            format!("expected exactly 1 author, got {}", arr.len()),
        );
    }

    pass(suite, name, request, Some(pstatus), duration)
}

// N-04 — `?includes=<field>` expands the nested entity inline.
//
// Note: the `includes` parameter is keyed by the FIELD name on the parent
// (here `author`), not by the sub-entity's name (see
// `internal/api/include.go:applyIncludesRecursive` →
// `collectIncludeFields`). After the include runs, `post.author` should be
// the full author document with `@entity` reattached, not just the
// `{@entity, id}` reference.
async fn n04_includes_expands_nested(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "N-04 includes expands nested";
    let start = Instant::now();

    wipe_all(client).await;

    let body = json!({
        "title": "Expand me",
        "author": {"@entity": AUTHORS, "fullname": "Dora"}
    });
    let resp = match client.create(POSTS, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                format!("GET /api/{POSTS}/{{id}}?includes=author"),
                None,
                start.elapsed(),
                format!("post create failed: {e:#}"),
            )
        }
    };
    let post: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                format!("GET /api/{POSTS}/{{id}}?includes=author"),
                None,
                start.elapsed(),
                format!("post JSON: {e:#}"),
            )
        }
    };
    let post_id = match post.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return fail(
                suite,
                name,
                format!("GET /api/{POSTS}/{{id}}?includes=author"),
                None,
                start.elapsed(),
                "post create missing id",
            )
        }
    };
    let request = format!("GET /api/{POSTS}/{post_id}?includes=author");

    let resp = match client
        .get_with_params(POSTS, &post_id, &[("includes", "author")])
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
                format!("get failed: {e:#}"),
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

    // After include: author must be an object with `fullname` resolved (i.e.
    // expanded), not just the `{@entity, id}` reference.
    let author = match body.get("author") {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                duration,
                format!("missing author field in: {body}"),
            )
        }
    };
    if author.get("fullname").and_then(|v| v.as_str()) != Some("Dora") {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("author was not expanded (no `fullname`): {author}"),
        );
    }
    if author.get("id").and_then(|v| v.as_str()).is_none() {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expanded author missing id: {author}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// N-05 — `?includes=all` recursively expands every @entity reference.
//
// The seed creates a Post → Author → Job chain; `includes=all` must walk to
// the deepest level and expand both layers.
async fn n05_includes_all(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "N-05 includes=all";
    let start = Instant::now();

    wipe_all(client).await;

    let body = json!({
        "title": "All",
        "author": {
            "@entity": AUTHORS,
            "fullname": "Eve",
            "job": {
                "@entity": JOBS,
                "title": "Designer"
            }
        }
    });
    let resp = match client.create(POSTS, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                format!("GET /api/{POSTS}/{{id}}?includes=all"),
                None,
                start.elapsed(),
                format!("post create failed: {e:#}"),
            )
        }
    };
    let post: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                format!("GET /api/{POSTS}/{{id}}?includes=all"),
                None,
                start.elapsed(),
                format!("post JSON: {e:#}"),
            )
        }
    };
    let post_id = match post.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return fail(
                suite,
                name,
                format!("GET /api/{POSTS}/{{id}}?includes=all"),
                None,
                start.elapsed(),
                "post create missing id",
            )
        }
    };
    let request = format!("GET /api/{POSTS}/{post_id}?includes=all");

    let resp = match client
        .get_with_params(POSTS, &post_id, &[("includes", "all")])
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
                format!("get failed: {e:#}"),
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

    if body.pointer("/author/fullname").and_then(|v| v.as_str()) != Some("Eve") {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("author level not expanded: {body}"),
        );
    }
    if body.pointer("/author/job/title").and_then(|v| v.as_str()) != Some("Designer") {
        return fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("job level not expanded recursively: {body}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}

// N-06 — Array of nested @entity objects each become separate documents.
//
// Per `internal/api/relationship.go:handleArraySubEntities`, each array
// element with `@entity` is extracted into its own sub-entity. The parent's
// array is replaced by the link list `[{@entity,id}, ...]`.
async fn n06_array_of_nested_entities(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "N-06 Array of nested entities";
    let request = format!("POST /api/{POSTS} {{comments:[{{@entity:{COMMENTS}}}, ...]}}");
    let start = Instant::now();

    wipe_all(client).await;

    let body = json!({
        "title": "Array",
        "comments": [
            {"@entity": COMMENTS, "text": "first"},
            {"@entity": COMMENTS, "text": "second"},
            {"@entity": COMMENTS, "text": "third"}
        ]
    });
    let resp = match client.create(POSTS, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("create failed: {e:#}"),
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
    let post: Value = match resp.json().await {
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

    let arr = match post.get("comments").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("comments missing or not an array: {post}"),
            )
        }
    };
    if arr.len() != 3 {
        return fail(
            suite,
            name,
            request,
            Some(status),
            start.elapsed(),
            format!("expected 3 comment refs in post, got {}", arr.len()),
        );
    }
    let mut ids = Vec::with_capacity(3);
    for (i, item) in arr.iter().enumerate() {
        let ent = item.get("@entity").and_then(|v| v.as_str()).unwrap_or("");
        if ent != COMMENTS {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("comment[{i}] @entity mismatch: expected `{COMMENTS}`, got `{ent}`"),
            );
        }
        let id = match item.get("id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return fail(
                    suite,
                    name,
                    request,
                    Some(status),
                    start.elapsed(),
                    format!("comment[{i}] missing id: {item}"),
                )
            }
        };
        ids.push(id);
    }

    // Cross-entity verification: each comment exists in COMMENTS and the
    // text values match what was sent.
    let resp = match client.list(COMMENTS, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("comments list failed: {e:#}"),
            )
        }
    };
    let lstatus = resp.status().as_u16();
    if lstatus != 200 {
        return fail(
            suite,
            name,
            request,
            Some(lstatus),
            start.elapsed(),
            format!("comments list expected 200, got {lstatus}"),
        );
    }
    let comments: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(lstatus),
                start.elapsed(),
                format!("comments JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let comments_arr = match comments.as_array() {
        Some(a) => a,
        None => {
            return fail(
                suite,
                name,
                request,
                Some(lstatus),
                duration,
                "comments list is not an array",
            )
        }
    };
    if comments_arr.len() != 3 {
        return fail(
            suite,
            name,
            request,
            Some(lstatus),
            duration,
            format!("expected 3 comments, got {}", comments_arr.len()),
        );
    }

    let mut texts: Vec<String> = comments_arr
        .iter()
        .filter_map(|d| d.get("text").and_then(|v| v.as_str()).map(String::from))
        .collect();
    texts.sort();
    let expected = vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
    ];
    if texts != expected {
        return fail(
            suite,
            name,
            request,
            Some(lstatus),
            duration,
            format!("comment texts mismatch: expected {expected:?}, got {texts:?}"),
        );
    }

    pass(suite, name, request, Some(status), duration)
}
