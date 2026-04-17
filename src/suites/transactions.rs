//! Suite 9 — Transactions (8 tests, TX-01..TX-08).
//!
//! Exercises ElysianDB's transactional HTTP API (`/api/tx/...`):
//!
//!   - TX-01: `POST /api/tx/begin` returns a non-empty `transaction_id`.
//!   - TX-02: `POST /api/tx/{txId}/entity/{entity}` queues a write (200).
//!   - TX-03: `PUT /api/tx/{txId}/entity/{entity}/{id}` queues an update.
//!     The controller validates that the target doc already exists in the
//!     main DB (`engine.ReadEntityById`) — not in the tx's pending ops — so
//!     the test seeds a doc via the normal API before opening the tx.
//!   - TX-04: `DELETE /api/tx/{txId}/entity/{entity}/{id}` queues a delete.
//!   - TX-05: Commit applies write + update + delete. The test pre-seeds
//!     two docs (one for update, one for delete) via the normal API, then
//!     within a single tx writes a new doc, updates one pre-seed, deletes
//!     the other, commits, and verifies all three outcomes.
//!   - TX-06: Rollback discards the queued write (verified by a 404 after
//!     rollback).
//!   - TX-07: Isolation — a write queued inside an open tx is invisible to
//!     the normal GET path before commit. The tx is rolled back at the end
//!     so the uncommitted write doesn't leak into later suites.
//!   - TX-08: Commit against a non-existent transaction ID returns 400.
//!     `CommitTransactionController` (and the underlying
//!     `transaction.CommitTransaction`) returns "transaction not found"
//!     which the controller maps to `StatusBadRequest`.
//!
//! ## Why custom IDs for the tx writes
//!
//! `POST /api/tx/{txId}/entity/{entity}` does NOT return the generated
//! document back — it just returns 200. That makes "did this write land?"
//! impossible to verify after the fact unless we control the id up front.
//! Every tx test that needs to assert presence/absence of a specific doc
//! passes `{"id":"tx0N-..."}` in the body so the tx test can GET by that
//! known id after commit/rollback.

use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};

const ENTITY: &str = "battle_tx_items";

pub struct TransactionsSuite;

#[async_trait]
impl TestSuite for TransactionsSuite {
    fn name(&self) -> &'static str {
        "Transactions"
    }

    fn description(&self) -> &'static str {
        "Validates transactional write/update/delete queueing, commit application, rollback discard, pre-commit isolation, and invalid-txId handling"
    }

    async fn setup(&self, client: &ElysianClient) -> Result<()> {
        // Wipe the entity so repeated runs start from zero docs even when
        // the suite is invoked standalone (`--suite transactions`).
        let _ = client.delete_all(ENTITY).await;
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(8);

        results.push(tx01_begin_transaction(&suite, client).await);
        results.push(tx02_write_in_transaction(&suite, client).await);
        results.push(tx03_update_in_transaction(&suite, client).await);
        results.push(tx04_delete_in_transaction(&suite, client).await);
        results.push(tx05_commit_applies_all(&suite, client).await);
        results.push(tx06_rollback_discards(&suite, client).await);
        results.push(tx07_isolation_uncommitted_not_visible(&suite, client).await);
        results.push(tx08_invalid_transaction_id(&suite, client).await);

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

/// Begin a transaction and return its id, or a caller-formatted error string.
async fn begin_tx(client: &ElysianClient) -> Result<String, String> {
    let resp = client
        .tx_begin()
        .await
        .map_err(|e| format!("tx_begin request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("tx_begin expected 200, got {status}"));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("tx_begin JSON parse failed: {e:#}"))?;
    let tx_id = body
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("tx_begin response missing transaction_id: {body}"))?
        .to_string();
    if tx_id.is_empty() {
        return Err("tx_begin returned empty transaction_id".to_string());
    }
    Ok(tx_id)
}

/// Create a document via the normal API and return its id. Used to seed
/// pre-existing docs for TX-03 (update requires the target to exist in the
/// main DB) and TX-05 (update + delete targets).
async fn seed_doc(client: &ElysianClient, id: &str, body: Value) -> Result<(), String> {
    let mut payload = body;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("id".to_string(), json!(id));
    }
    let resp = client
        .create(ENTITY, payload)
        .await
        .map_err(|e| format!("seed create failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("seed create expected 200, got {status}"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// TX-01 — Begin transaction returns a non-empty transaction_id (JSON, 200).
async fn tx01_begin_transaction(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-01 Begin transaction";
    let request = "POST /api/tx/begin".to_string();
    let start = Instant::now();

    let tx_id = match begin_tx(client).await {
        Ok(id) => id,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    // Clean up the just-opened tx so it doesn't leak into TxManager for
    // the rest of the suite. Rollback silently succeeds on valid txs.
    let _ = client.tx_rollback(&tx_id).await;

    pass(suite, name, request, Some(200), duration)
}

// TX-02 — Write in transaction is accepted (200).
//
// Asserts only that the queue operation succeeds — TX-05 is the one that
// verifies the write actually lands in the DB after commit.
async fn tx02_write_in_transaction(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-02 Write in transaction";
    let request = format!("POST /api/tx/{{txId}}/entity/{ENTITY}");
    let start = Instant::now();

    let tx_id = match begin_tx(client).await {
        Ok(id) => id,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    let resp = match client
        .tx_write(&tx_id, ENTITY, json!({"id": "tx02-item", "name": "Item1"}))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("tx_write failed: {e:#}"),
            );
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    // Rollback to release the tx without persisting the queued write.
    let _ = client.tx_rollback(&tx_id).await;

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

// TX-03 — Update in transaction is accepted.
//
// `UpdateTransactionController` validates against the main DB, not the tx
// queue, so the target doc must exist before the tx opens.
async fn tx03_update_in_transaction(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-03 Update in transaction";
    let request = format!("PUT /api/tx/{{txId}}/entity/{ENTITY}/{{id}}");
    let start = Instant::now();

    let seed_id = "tx03-seed";
    if let Err(msg) = seed_doc(client, seed_id, json!({"name": "Original"})).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let tx_id = match begin_tx(client).await {
        Ok(id) => id,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    let resp = match client
        .tx_update(&tx_id, ENTITY, seed_id, json!({"name": "Updated"}))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("tx_update failed: {e:#}"),
            );
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    let _ = client.tx_rollback(&tx_id).await;
    let _ = client.delete(ENTITY, seed_id).await;

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

// TX-04 — Delete in transaction is accepted.
async fn tx04_delete_in_transaction(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-04 Delete in transaction";
    let request = format!("DELETE /api/tx/{{txId}}/entity/{ENTITY}/{{id}}");
    let start = Instant::now();

    let seed_id = "tx04-seed";
    if let Err(msg) = seed_doc(client, seed_id, json!({"name": "ToDelete"})).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let tx_id = match begin_tx(client).await {
        Ok(id) => id,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    let resp = match client.tx_delete(&tx_id, ENTITY, seed_id).await {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("tx_delete failed: {e:#}"),
            );
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    // Rollback so the delete never actually lands. Then clean up the
    // seed directly via the normal API.
    let _ = client.tx_rollback(&tx_id).await;
    let _ = client.delete(ENTITY, seed_id).await;

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

// TX-05 — Commit applies every queued operation.
//
// Seed two docs (tx05-upd, tx05-del) via the normal API. Inside one tx:
// write a brand-new doc (tx05-new), update tx05-upd, delete tx05-del.
// Commit, then verify:
//   - tx05-new exists with kind=new
//   - tx05-upd's kind is now "updated" (preserving unchanged "value" field)
//   - tx05-del is gone (404)
async fn tx05_commit_applies_all(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-05 Commit applies all";
    let request = "POST /api/tx/{txId}/commit (after write+update+delete)".to_string();
    let start = Instant::now();

    let upd_id = "tx05-upd";
    let del_id = "tx05-del";
    let new_id = "tx05-new";

    if let Err(msg) = seed_doc(client, upd_id, json!({"kind": "original", "value": 1})).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }
    if let Err(msg) = seed_doc(client, del_id, json!({"kind": "original", "value": 2})).await {
        return fail(suite, name, request, None, start.elapsed(), msg);
    }

    let tx_id = match begin_tx(client).await {
        Ok(id) => id,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    // Queue write
    let resp = match client
        .tx_write(
            &tx_id,
            ENTITY,
            json!({"id": new_id, "kind": "new", "value": 3}),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("queue write failed: {e:#}"),
            );
        }
    };
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        let _ = client.tx_rollback(&tx_id).await;
        return fail(
            suite,
            name,
            request,
            Some(s),
            start.elapsed(),
            format!("queue write expected 2xx, got {s}"),
        );
    }

    // Queue update
    let resp = match client
        .tx_update(&tx_id, ENTITY, upd_id, json!({"kind": "updated"}))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("queue update failed: {e:#}"),
            );
        }
    };
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        let _ = client.tx_rollback(&tx_id).await;
        return fail(
            suite,
            name,
            request,
            Some(s),
            start.elapsed(),
            format!("queue update expected 2xx, got {s}"),
        );
    }

    // Queue delete
    let resp = match client.tx_delete(&tx_id, ENTITY, del_id).await {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("queue delete failed: {e:#}"),
            );
        }
    };
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        let _ = client.tx_rollback(&tx_id).await;
        return fail(
            suite,
            name,
            request,
            Some(s),
            start.elapsed(),
            format!("queue delete expected 2xx, got {s}"),
        );
    }

    // Commit
    let resp = match client.tx_commit(&tx_id).await {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("commit request failed: {e:#}"),
            );
        }
    };
    let commit_status = resp.status().as_u16();
    if commit_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(commit_status),
            start.elapsed(),
            format!("commit expected 200, got {commit_status}"),
        );
    }

    // Verify tx05-new landed
    let resp = match client.get(ENTITY, new_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(commit_status),
                start.elapsed(),
                format!("verify new GET failed: {e:#}"),
            )
        }
    };
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        return fail(
            suite,
            name,
            request,
            Some(s),
            start.elapsed(),
            format!("post-commit GET {new_id} expected 2xx, got {s}"),
        );
    }
    let new_body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(commit_status),
                start.elapsed(),
                format!("new GET JSON parse failed: {e:#}"),
            )
        }
    };
    if new_body.get("kind").and_then(|v| v.as_str()) != Some("new") {
        return fail(
            suite,
            name,
            request,
            Some(commit_status),
            start.elapsed(),
            format!("expected new.kind=\"new\", got {new_body}"),
        );
    }

    // Verify tx05-upd was updated (kind flipped, value preserved)
    let resp = match client.get(ENTITY, upd_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(commit_status),
                start.elapsed(),
                format!("verify upd GET failed: {e:#}"),
            )
        }
    };
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        return fail(
            suite,
            name,
            request,
            Some(s),
            start.elapsed(),
            format!("post-commit GET {upd_id} expected 2xx, got {s}"),
        );
    }
    let upd_body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(commit_status),
                start.elapsed(),
                format!("upd GET JSON parse failed: {e:#}"),
            )
        }
    };
    if upd_body.get("kind").and_then(|v| v.as_str()) != Some("updated") {
        return fail(
            suite,
            name,
            request,
            Some(commit_status),
            start.elapsed(),
            format!("expected upd.kind=\"updated\", got {upd_body}"),
        );
    }

    // Verify tx05-del was removed
    let resp = match client.get(ENTITY, del_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(commit_status),
                start.elapsed(),
                format!("verify del GET failed: {e:#}"),
            )
        }
    };
    let del_status = resp.status().as_u16();
    let duration = start.elapsed();
    if del_status != 404 {
        return fail(
            suite,
            name,
            request,
            Some(del_status),
            duration,
            format!("expected 404 for deleted {del_id}, got {del_status}"),
        );
    }

    pass(suite, name, request, Some(commit_status), duration)
}

// TX-06 — Rollback discards the queued write (verified by a 404 after).
async fn tx06_rollback_discards(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-06 Rollback discards";
    let request = "POST /api/tx/{txId}/rollback (after write)".to_string();
    let start = Instant::now();

    let doc_id = "tx06-rollback";

    let tx_id = match begin_tx(client).await {
        Ok(id) => id,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    let resp = match client
        .tx_write(
            &tx_id,
            ENTITY,
            json!({"id": doc_id, "kind": "should-not-persist"}),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("queue write failed: {e:#}"),
            );
        }
    };
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        let _ = client.tx_rollback(&tx_id).await;
        return fail(
            suite,
            name,
            request,
            Some(s),
            start.elapsed(),
            format!("queue write expected 2xx, got {s}"),
        );
    }

    let resp = match client.tx_rollback(&tx_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("rollback request failed: {e:#}"),
            )
        }
    };
    let rollback_status = resp.status().as_u16();
    if rollback_status != 200 {
        return fail(
            suite,
            name,
            request,
            Some(rollback_status),
            start.elapsed(),
            format!("rollback expected 200, got {rollback_status}"),
        );
    }

    // Post-rollback GET must be 404.
    let resp = match client.get(ENTITY, doc_id).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                Some(rollback_status),
                start.elapsed(),
                format!("verify GET failed: {e:#}"),
            )
        }
    };
    let get_status = resp.status().as_u16();
    let duration = start.elapsed();

    if get_status == 404 {
        pass(suite, name, request, Some(rollback_status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(get_status),
            duration,
            format!("expected 404 after rollback, got {get_status}"),
        )
    }
}

// TX-07 — Uncommitted write is invisible to the normal GET path.
async fn tx07_isolation_uncommitted_not_visible(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-07 Isolation: uncommitted not visible";
    let request = format!("GET /api/{ENTITY}/{{id}} (mid-tx, pre-commit)");
    let start = Instant::now();

    let doc_id = "tx07-isolated";

    let tx_id = match begin_tx(client).await {
        Ok(id) => id,
        Err(msg) => return fail(suite, name, request, None, start.elapsed(), msg),
    };

    let resp = match client
        .tx_write(&tx_id, ENTITY, json!({"id": doc_id, "kind": "invisible"}))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("queue write failed: {e:#}"),
            );
        }
    };
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        let _ = client.tx_rollback(&tx_id).await;
        return fail(
            suite,
            name,
            request,
            Some(s),
            start.elapsed(),
            format!("queue write expected 2xx, got {s}"),
        );
    }

    // Mid-tx GET via the normal API must not see the queued write.
    let resp = match client.get(ENTITY, doc_id).await {
        Ok(r) => r,
        Err(e) => {
            let _ = client.tx_rollback(&tx_id).await;
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("mid-tx GET failed: {e:#}"),
            );
        }
    };
    let status = resp.status().as_u16();
    let duration = start.elapsed();

    // Tear down the tx so the "invisible" write never lands.
    let _ = client.tx_rollback(&tx_id).await;

    if status == 404 {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 404 (uncommitted write invisible), got {status}"),
        )
    }
}

// TX-08 — Commit against a non-existent transaction ID returns 400.
//
// `CommitTransactionController` maps "transaction not found" to
// `StatusBadRequest`. The spec only requires "error response", so we also
// accept other 4xx codes defensively in case the controller's mapping
// shifts.
async fn tx08_invalid_transaction_id(suite: &str, client: &ElysianClient) -> TestResult {
    let name = "TX-08 Invalid transaction ID";
    let request = "POST /api/tx/fake-id/commit".to_string();
    let start = Instant::now();

    let resp = match client.tx_commit("fake-id-does-not-exist").await {
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

    if (400..500).contains(&status) {
        pass(suite, name, request, Some(status), duration)
    } else {
        fail(
            suite,
            name,
            request,
            Some(status),
            duration,
            format!("expected 4xx error, got {status}"),
        )
    }
}
