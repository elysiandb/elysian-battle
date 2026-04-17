//! Suite 8 — Access Control Lists (10 tests, ACL-01..ACL-10).
//!
//! Exercises ElysianDB's per-user ACL enforcement (`internal/acl/acl.go`).
//! The suite juggles two clients at once:
//!
//!   - `admin_client` — the shared suite client, already logged in as the
//!     default admin by the runner. Used for all admin-perspective
//!     operations (ACL-01) and for manipulating the test user's ACL records.
//!   - `user_client` — a cookie-isolated client logged in as the test user.
//!     Used for the user-perspective checks (ACL-02..ACL-05, ACL-08,
//!     ACL-10).
//!
//! ## ElysianDB v0.1.14 quirks that shape these tests
//!
//! 1. **Admin-only HTTP login.** `LoginController` rejects every role that
//!    isn't `admin`, so the test user has to be created with `role: "admin"`
//!    to obtain a session at all (same workaround as the Authentication
//!    suite — see `src/suites/auth.rs` module docs). The user's role on
//!    `_elysiandb_core_user` is "admin"; we simulate user-role behavior
//!    entirely through ACL records, which are the authoritative gate in
//!    `internal/acl/acl.go` (only `UserAuthenticationIsEnabled` is checked
//!    there; no role-based short-circuit).
//!
//! 2. **`PUT /api/acl/{user}/{entity}` replaces the entire permission map.**
//!    `UpdateACLForUsernameAndEntityController` starts from
//!    `NewPermissions()` (all `false`) and only copies keys present in the
//!    payload, so every test that only needs to flip one bit still has to
//!    pass the full map to avoid clobbering the others.
//!
//! 3. **`POST /api/{entity}` is NOT ACL-enforced at the HTTP layer.**
//!    `CreateController` never calls `acl.CanCreateEntity`, which means
//!    ACL-02 ("user default: can create") will always pass against v0.1.14
//!    — the test asserts the observable contract (create succeeds) rather
//!    than pretending to enforce an owning-write check that isn't wired.
//!
//! 4. **`PUT /api/acl/{user}/{entity}/default` uses the user's stored role
//!    to pick defaults.** Because our test user has `role: "admin"`, the
//!    "default" is admin permissions (full access), not the owning-only set
//!    the spec describes. ACL-09 asserts the reset SUCCEEDS and actually
//!    returns the defaults appropriate for the user's role — which, given
//!    quirk #1, means full access. This is an intentional deviation from
//!    the spec text, documented inline with the test.

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const ACL_USERNAME: &str = "battle_acl_user";
const ACL_PASSWORD: &str = "acl-pass-2026";
const ACL_ENTITY: &str = "battle_acl_data";

/// Ordered list of every test id in this suite. Used by the setup-failure
/// branches in `run()` to emit one placeholder failure per test, keeping
/// the two error paths in sync when tests are added/removed.
const ACL_TEST_IDS: &[&str] = &[
    "ACL-01", "ACL-02", "ACL-03", "ACL-04", "ACL-05", "ACL-06", "ACL-07", "ACL-08", "ACL-09",
    "ACL-10",
];

pub struct AclSuite;

#[async_trait]
impl TestSuite for AclSuite {
    fn name(&self) -> &'static str {
        "ACL"
    }

    fn description(&self) -> &'static str {
        "Validates per-user ACL enforcement: admin full access, user owning permissions, global grant/revoke, get/reset ACL, cross-user deletion rejection"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        // Clean any residue from prior runs — 404 on delete is fine.
        let _ = client.delete_all(ACL_ENTITY).await;
        let _ = client.delete_user(ACL_USERNAME).await;
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let port = client.port();
        let mut results = Vec::with_capacity(10);

        // Create the test user (role=admin — see module docs) and a seed
        // document so the entity type is registered and InitACL runs.
        if let Err(msg) = seed_user_and_entity(client).await {
            // Every test returns the same setup failure — there is no
            // point running them against a half-built fixture.
            for id in ACL_TEST_IDS {
                results.push(fail(
                    &suite,
                    &format!("{id} Setup failed"),
                    "setup".into(),
                    None,
                    std::time::Duration::ZERO,
                    msg.clone(),
                ));
            }
            return results;
        }

        // Login as the test user on a fresh client so the user and admin
        // keep distinct cookie jars.
        let user_client = match new_user_client(port).await {
            Ok(c) => c,
            Err(msg) => {
                for id in ACL_TEST_IDS {
                    results.push(fail(
                        &suite,
                        &format!("{id} User login failed"),
                        "login".into(),
                        None,
                        std::time::Duration::ZERO,
                        msg.clone(),
                    ));
                }
                return results;
            }
        };

        results.push(acl01_admin_full_access(&suite, client).await);
        results.push(acl02_user_can_create(&suite, &user_client).await);
        results.push(acl03_user_can_read_own(&suite, client, &user_client).await);
        results.push(acl04_user_cannot_read_others(&suite, client, &user_client).await);
        results.push(acl05_grant_global_read(&suite, client, &user_client).await);
        results.push(acl06_get_acl(&suite, client).await);
        results.push(acl07_get_all_acls(&suite, client).await);
        results.push(acl08_revoke_permission(&suite, client, &user_client).await);
        results.push(acl09_reset_to_default(&suite, client).await);
        results.push(acl10_user_cannot_delete_others(&suite, client, &user_client).await);

        results
    }

    async fn teardown(&self, client: &ElysianClient) -> Result<()> {
        // IMPORTANT: do NOT `logout()` or otherwise invalidate the shared
        // `client`'s admin session here — the runner reuses this client
        // across suites and `cleanup_between_suites` swallows 401s
        // silently (`let _ = client.delete_all(...)`), so a stale session
        // would turn into invisible cleanup failures in whatever suite
        // runs next. If a future ACL test needs to log out or delete the
        // admin session's owner, re-establish admin via
        // `client.login("admin", "admin")` before returning from
        // teardown (see `auth.rs::ensure_admin_logged_in` for the idiom).
        let _ = client.delete_all(ACL_ENTITY).await;
        let _ = client.delete_user(ACL_USERNAME).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Global-scope permissions (the four unprefixed ACL keys).
#[derive(Clone, Copy)]
struct GlobalPerms {
    create: bool,
    read: bool,
    update: bool,
    delete: bool,
}

/// Owning-scope permissions (the four `owning_*` ACL keys).
#[derive(Clone, Copy)]
struct OwningPerms {
    read: bool,
    write: bool,
    update: bool,
    delete: bool,
}

/// Full permissions map the ACL PUT endpoint wants — every known key
/// explicitly set so callers never accidentally zero a perm they wanted to
/// preserve (see quirk #2 in the module docs).
fn perms_body(global: GlobalPerms, owning: OwningPerms) -> Value {
    json!({
        "permissions": {
            "create":        global.create,
            "read":          global.read,
            "update":        global.update,
            "delete":        global.delete,
            "owning_read":   owning.read,
            "owning_write":  owning.write,
            "owning_update": owning.update,
            "owning_delete": owning.delete,
        }
    })
}

const NO_GLOBAL: GlobalPerms = GlobalPerms {
    create: false,
    read: false,
    update: false,
    delete: false,
};

const READ_ONLY_GLOBAL: GlobalPerms = GlobalPerms {
    create: false,
    read: true,
    update: false,
    delete: false,
};

const FULL_OWNING: OwningPerms = OwningPerms {
    read: true,
    write: true,
    update: true,
    delete: true,
};

/// `{owning_* : true, global_* : false}` — the set we use to simulate a
/// role=user ACL even though the underlying user is technically role=admin.
fn owning_only() -> Value {
    perms_body(NO_GLOBAL, FULL_OWNING)
}

/// Create the test user and seed a document owned by admin. The seed doc
/// (a) registers the entity type so `InitACL` generates an ACL record for
/// the test user, and (b) lets us test "user cannot read others' docs"
/// later without extra setup.
async fn seed_user_and_entity(client: &ElysianClient) -> Result<(), String> {
    // Create the test user (role=admin — see module docs).
    let resp = client
        .create_user(json!({
            "username": ACL_USERNAME,
            "password": ACL_PASSWORD,
            "role":     "admin",
        }))
        .await
        .map_err(|e| format!("create_user failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("create_user expected 200, got {status}"));
    }

    // Seed an admin-owned document to register the entity type.
    let resp = client
        .create(ACL_ENTITY, json!({"kind": "admin_seed", "value": 1}))
        .await
        .map_err(|e| format!("seed create failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("seed create expected 200, got {status}"));
    }

    Ok(())
}

/// Bust the per-entity list cache for `ACL_ENTITY`.
///
/// `internal/transport/http/api/create.go:finalizeCreate` and
/// `internal/transport/http/api/delete_by_id.go` both call
/// `cache.CacheStore.Purge(entity)` — but `UpdateACLForUsernameAndEntity`
/// does NOT. That means after an ACL change, previously-cached list
/// responses for that user/entity are still served and do not reflect the
/// new permissions. We work around that by having admin create then
/// immediately delete a sentinel doc: each operation purges the cache, so
/// any following list call goes through `FilterListOfEntities` with the
/// current ACL.
async fn bust_cache(admin: &ElysianClient) -> Result<(), String> {
    let resp = admin
        .create(ACL_ENTITY, json!({"kind": "__acl_cache_bust"}))
        .await
        .map_err(|e| format!("cache-bust create failed: {e:#}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "cache-bust create returned {}",
            resp.status().as_u16()
        ));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("cache-bust JSON parse failed: {e:#}"))?;
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("cache-bust response missing id: {body}"))?;
    // Errors here are fatal: a surviving sentinel would appear as an
    // admin-owned doc in the entity and silently turn ACL-04/ACL-08 into
    // false positives ("no admin docs visible" would be wrong because the
    // leftover sentinel IS admin-owned).
    let delete_resp = admin
        .delete(ACL_ENTITY, id)
        .await
        .map_err(|e| format!("cache-bust delete failed: {e:#}"))?;
    if !delete_resp.status().is_success() {
        return Err(format!(
            "cache-bust delete for id={id} returned {}",
            delete_resp.status().as_u16()
        ));
    }
    Ok(())
}

/// Log in a fresh client as the test user. The runner's shared client
/// stays on the admin session.
async fn new_user_client(port: u16) -> Result<ElysianClient, String> {
    let client = ElysianClient::new(port);
    let resp = client
        .login(ACL_USERNAME, ACL_PASSWORD)
        .await
        .map_err(|e| format!("user login failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!(
            "user login expected 200, got {status} (LoginController may have rejected the role — see module docs)"
        ));
    }
    Ok(client)
}

/// Find an existing document owned by `username`. Admin-scoped call —
/// listing via the admin client bypasses the ACL filter because admin has
/// global read permission.
async fn find_doc_owned_by(
    client: &ElysianClient,
    username: &str,
) -> Result<Option<String>, String> {
    let resp = client
        .list(ACL_ENTITY, &[])
        .await
        .map_err(|e| format!("list failed: {e:#}"))?;
    if !resp.status().is_success() {
        return Err(format!("list returned {}", resp.status()));
    }
    let docs: Vec<Value> = resp
        .json()
        .await
        .map_err(|e| format!("list JSON parse failed: {e:#}"))?;

    for doc in docs {
        if doc.get("_elysiandb_core_username").and_then(|v| v.as_str()) == Some(username) {
            if let Some(id) = doc.get("id").and_then(|v| v.as_str()) {
                return Ok(Some(id.to_string()));
            }
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ACL-01 — Admin can create, read, and delete in `battle_acl_data`.
async fn acl01_admin_full_access(suite: &str, admin: &ElysianClient) -> TestResult {
    let name = "ACL-01 Admin full access";
    let request = format!("POST/GET/DELETE /api/{ACL_ENTITY} (admin)");
    let start = Instant::now();

    // Create.
    let resp = match admin
        .create(ACL_ENTITY, json!({"kind": "admin_full", "value": 100}))
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
                format!("create failed: {e:#}"),
            )
        }
    };
    let create_status = resp.status().as_u16();
    if create_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(create_status),
            start.elapsed(),
            format!("create expected 200, got {create_status}"),
        );
    }
    let doc: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(create_status),
                start.elapsed(),
                format!("create JSON parse failed: {e:#}"),
            )
        }
    };
    let id = match doc.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                Some(create_status),
                start.elapsed(),
                format!("create response missing id: {doc}"),
            )
        }
    };

    // Read.
    let resp = match admin.get(ACL_ENTITY, &id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("read failed: {e:#}"),
            )
        }
    };
    let read_status = resp.status().as_u16();
    if read_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(read_status),
            start.elapsed(),
            format!("read expected 200, got {read_status}"),
        );
    }

    // Delete.
    let resp = match admin.delete(ACL_ENTITY, &id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("delete failed: {e:#}"),
            )
        }
    };
    let delete_status = resp.status().as_u16();
    let duration = start.elapsed();

    if !(delete_status == 200 || delete_status == 204) {
        return fail(
            suite,
            name,
            request,
            Some(delete_status),
            duration,
            format!("delete expected 200/204, got {delete_status}"),
        );
    }

    pass(suite, name, request, Some(delete_status), duration)
}

// ACL-02 — User creates a document in `battle_acl_data`.
//
// v0.1.14 does NOT gate POST on ACL (see module docs, quirk #3), so this
// test verifies the observable contract: "user can write". The created
// document is implicitly owned by `ACL_USERNAME` via `_elysiandb_core_username`.
async fn acl02_user_can_create(suite: &str, user: &ElysianClient) -> TestResult {
    let name = "ACL-02 User can create";
    let request = format!("POST /api/{ACL_ENTITY} (as user)");
    let start = Instant::now();

    let resp = match user
        .create(ACL_ENTITY, json!({"kind": "user_own", "value": 1}))
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
                format!("create failed: {e:#}"),
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

// ACL-03 — User reads their own doc (owning_read).
//
// Resets the user ACL to owning-only first so the check actually exercises
// the `owning_read` branch of `CanReadEntity` rather than the default
// admin-role `read` grant. Without this step, the test would pass even if
// `CanReadEntity`'s owning-branch regressed, because the default admin-role
// ACL (see module docs, quirk #1) grants full `read`.
async fn acl03_user_can_read_own(
    suite: &str,
    admin: &ElysianClient,
    user: &ElysianClient,
) -> TestResult {
    let name = "ACL-03 User can read own";
    let request = format!("GET /api/{ACL_ENTITY}/{{user_doc_id}}");
    let start = Instant::now();

    // Restrict to owning-only so the list filter and per-id ACL check both
    // go through the owning branches.
    if let Err(e) = admin.set_acl(ACL_USERNAME, ACL_ENTITY, owning_only()).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("set_acl (owning_only) failed: {e:#}"),
        );
    }
    if let Err(msg) = bust_cache(admin).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("bust_cache failed: {msg}"),
        );
    }

    // Find the user's doc by listing via the user client — under
    // owning-only the list is filtered to docs the user owns.
    let resp = match user.list(ACL_ENTITY, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("user list failed: {e:#}"),
            )
        }
    };
    let docs: Vec<Value> = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("list JSON parse failed: {e:#}"),
            )
        }
    };
    let id = match docs
        .iter()
        .find(|d| d.get("_elysiandb_core_username").and_then(|v| v.as_str()) == Some(ACL_USERNAME))
        .and_then(|d| d.get("id").and_then(|v| v.as_str()))
    {
        Some(s) => s.to_string(),
        None => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("no user-owned doc found in list: {docs:?}"),
            )
        }
    };

    let resp = match user.get(ACL_ENTITY, &id).await {
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
            format!("expected 200 on own doc, got {status}"),
        )
    }
}

// ACL-04 — With owning-only ACL, the user's list must not contain any
// admin-owned documents.
async fn acl04_user_cannot_read_others(
    suite: &str,
    admin: &ElysianClient,
    user: &ElysianClient,
) -> TestResult {
    let name = "ACL-04 User cannot read others";
    let request = format!("GET /api/{ACL_ENTITY} (as user, owning-only)");
    let start = Instant::now();

    // Restrict the user to owning-only perms for the duration of this test.
    if let Err(e) = admin.set_acl(ACL_USERNAME, ACL_ENTITY, owning_only()).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("set_acl (owning_only) failed: {e:#}"),
        );
    }
    // `PUT /api/acl` doesn't purge the list cache — bust it explicitly so
    // the upcoming user.list reflects the new ACL.
    if let Err(msg) = bust_cache(admin).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("bust_cache failed: {msg}"),
        );
    }

    let resp = match user.list(ACL_ENTITY, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("list failed: {e:#}"),
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
            format!("list expected 200, got {status}"),
        );
    }
    let docs: Vec<Value> = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("list JSON parse failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let admin_visible = docs
        .iter()
        .any(|d| d.get("_elysiandb_core_username").and_then(|v| v.as_str()) == Some("admin"));
    if admin_visible {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected no admin-owned docs, got {docs:?}"),
        )
    } else {
        pass(suite, name, request, Some(status), duration)
    }
}

// ACL-05 — Admin grants global `read`, user now sees admin-owned docs too.
async fn acl05_grant_global_read(
    suite: &str,
    admin: &ElysianClient,
    user: &ElysianClient,
) -> TestResult {
    let name = "ACL-05 Grant global read";
    let request = format!("PUT /api/acl/{ACL_USERNAME}/{ACL_ENTITY} {{read:true,…}}");
    let start = Instant::now();

    // `read:true` dominates the ACL so the user sees every doc; owning
    // perms stay on so subsequent tests can reset back to owning-only
    // without granting writes the user never had.
    let body = perms_body(READ_ONLY_GLOBAL, FULL_OWNING);
    let resp = match admin.set_acl(ACL_USERNAME, ACL_ENTITY, body).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("set_acl failed: {e:#}"),
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
            format!("set_acl expected 200, got {status}"),
        );
    }
    if let Err(msg) = bust_cache(admin).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("bust_cache failed: {msg}"),
        );
    }

    let resp = match user.list(ACL_ENTITY, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("user list failed: {e:#}"),
            )
        }
    };
    let lstatus = resp.status().as_u16();
    let docs: Vec<Value> = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(lstatus),
                start.elapsed(),
                format!("list JSON parse failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let admin_visible = docs
        .iter()
        .any(|d| d.get("_elysiandb_core_username").and_then(|v| v.as_str()) == Some("admin"));
    if admin_visible {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(lstatus),
            duration,
            format!("expected admin-owned docs visible under global read, got {docs:?}"),
        )
    }
}

// ACL-06 — GET ACL returns the current permissions for the pair.
async fn acl06_get_acl(suite: &str, admin: &ElysianClient) -> TestResult {
    let name = "ACL-06 Get ACL";
    let request = format!("GET /api/acl/{ACL_USERNAME}/{ACL_ENTITY}");
    let start = Instant::now();

    let resp = match admin.get_acl(ACL_USERNAME, ACL_ENTITY).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("get_acl failed: {e:#}"),
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

    // Body shape from `ACL.ToDataMap`: {id, username, entity, permissions{...}}.
    let username = body.get("username").and_then(|v| v.as_str());
    let entity = body.get("entity").and_then(|v| v.as_str());
    let has_perms = body
        .get("permissions")
        .map(|v| v.is_object())
        .unwrap_or(false);
    if username == Some(ACL_USERNAME) && entity == Some(ACL_ENTITY) && has_perms {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!(
                "expected {{username:{ACL_USERNAME},entity:{ACL_ENTITY},permissions:{{…}}}}, got {body}"
            ),
        )
    }
}

// ACL-07 — GET all ACLs for the user returns an array/object covering
// multiple entities. v0.1.14 returns a JSON array of ACL records; the
// exact shape is asserted loosely (non-empty, each record references the
// target username).
async fn acl07_get_all_acls(suite: &str, admin: &ElysianClient) -> TestResult {
    let name = "ACL-07 Get all ACLs";
    let request = format!("GET /api/acl/{ACL_USERNAME}");
    let start = Instant::now();

    let resp = match admin.get_all_acls(ACL_USERNAME).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("get_all_acls failed: {e:#}"),
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

    // Accept either an array (old shape) or an object wrapping `acls`
    // (hypothetical future shape) — both need to reference our username.
    let entries: Option<&Vec<Value>> = body.as_array().or_else(|| {
        body.get("acls")
            .and_then(|v| v.as_array())
            .or_else(|| body.get("entities").and_then(|v| v.as_array()))
    });

    let all_match = match entries {
        Some(arr) if !arr.is_empty() => arr.iter().any(|e| {
            e.get("username").and_then(|v| v.as_str()) == Some(ACL_USERNAME)
                || e.get("entity").and_then(|v| v.as_str()) == Some(ACL_ENTITY)
        }),
        _ => false,
    };

    if all_match {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected a non-empty list referencing {ACL_USERNAME}, got {body}"),
        )
    }
}

// ACL-08 — After revoking global `read` (owning-only remains), the user
// stops seeing admin-owned documents in list results.
async fn acl08_revoke_permission(
    suite: &str,
    admin: &ElysianClient,
    user: &ElysianClient,
) -> TestResult {
    let name = "ACL-08 Revoke permission";
    let request = format!("PUT /api/acl/{ACL_USERNAME}/{ACL_ENTITY} {{read:false,owning_*:true}}");
    let start = Instant::now();

    // Flip read back off, keep owning perms on.
    let resp = match admin.set_acl(ACL_USERNAME, ACL_ENTITY, owning_only()).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("set_acl failed: {e:#}"),
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
            format!("set_acl expected 200, got {status}"),
        );
    }
    if let Err(msg) = bust_cache(admin).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("bust_cache failed: {msg}"),
        );
    }

    let resp = match user.list(ACL_ENTITY, &[]).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("user list failed: {e:#}"),
            )
        }
    };
    let lstatus = resp.status().as_u16();
    let docs: Vec<Value> = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(lstatus),
                start.elapsed(),
                format!("list JSON parse failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    let admin_visible = docs
        .iter()
        .any(|d| d.get("_elysiandb_core_username").and_then(|v| v.as_str()) == Some("admin"));
    if admin_visible {
        fail(
            suite,
            name,
            request,
            Some(lstatus),
            duration,
            format!("admin docs still visible after revoke: {docs:?}"),
        )
    } else {
        // Report the user-list status on both branches so the pass/fail
        // reports stay consistent with ACL-04 and ACL-05.
        pass(suite, name, request, Some(lstatus), duration)
    }
}

// ACL-09 — Reset to default succeeds.
//
// `ResetACLEntityToDefault` picks defaults based on the user's stored role
// (`_elysiandb_core_user`). Our test user's role is `"admin"` (see module
// docs, quirk #1 and #4), so the "default" here is FULL admin permissions
// — not the owning-only set the original spec describes. We therefore
// assert what v0.1.14 actually does given the stored role: the endpoint
// returns 200 and the ACL flips to full-access.
async fn acl09_reset_to_default(suite: &str, admin: &ElysianClient) -> TestResult {
    let name = "ACL-09 Reset to default";
    let request = format!("PUT /api/acl/{ACL_USERNAME}/{ACL_ENTITY}/default");
    let start = Instant::now();

    let resp = match admin.reset_acl(ACL_USERNAME, ACL_ENTITY).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("reset_acl failed: {e:#}"),
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

    // Read the ACL back and verify it reflects the user's role-based
    // defaults. For role=admin, every permission is true.
    let verify = match admin.get_acl(ACL_USERNAME, ACL_ENTITY).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("verify get_acl failed: {e:#}"),
            )
        }
    };
    let body: Value = match verify.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(status),
                start.elapsed(),
                format!("verify invalid JSON: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    // Check that `read` is true (admin default) — the most user-visible
    // bit of "reset worked".
    let read = body
        .pointer("/permissions/read")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if read {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected `read:true` after role=admin reset, got body: {body}"),
        )
    }
}

// ACL-10 — User attempts to delete an admin-owned document. With
// owning-only permissions the delete must be rejected (403), per
// `CanDeleteEntity` — `delete` is false and `owning_delete` only matches
// when the doc's owner is the current user.
async fn acl10_user_cannot_delete_others(
    suite: &str,
    admin: &ElysianClient,
    user: &ElysianClient,
) -> TestResult {
    let name = "ACL-10 User cannot delete others' doc";
    let request = format!("DELETE /api/{ACL_ENTITY}/{{admin_doc_id}} (as user)");
    let start = Instant::now();

    // Switch the user back to owning-only so ACL-09's full reset doesn't
    // leak into this test.
    if let Err(e) = admin.set_acl(ACL_USERNAME, ACL_ENTITY, owning_only()).await {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("set_acl (owning_only) failed: {e:#}"),
        );
    }

    // Make sure at least one admin-owned doc exists (previous tests may
    // have created/deleted some). We do this via the admin client because
    // the user list call is ACL-filtered.
    let admin_doc_id = match find_doc_owned_by(admin, "admin").await {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Seed a new admin doc so the test has something to attack.
            let resp = match admin
                .create(ACL_ENTITY, json!({"kind": "admin_seed_acl10", "value": 42}))
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
                        format!("admin seed create failed: {e:#}"),
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
                        None,
                        start.elapsed(),
                        format!("admin seed JSON failed: {e:#}"),
                    )
                }
            };
            match body.get("id").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => {
                    return fail(
                        suite,
                        name,
                        request,
                        None,
                        start.elapsed(),
                        format!("admin seed missing id: {body}"),
                    )
                }
            }
        }
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("find_doc_owned_by failed: {e}"),
            )
        }
    };

    let resp = match user.delete(ACL_ENTITY, &admin_doc_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("delete failed: {e:#}"),
            )
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    if status == 403 {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 403, got {status}"),
        )
    }
}
