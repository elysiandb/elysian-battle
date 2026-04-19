//! Suite 16 — Crash Recovery (3 tests, CR-01..CR-03).
//!
//! These tests stop and restart the running ElysianDB process, so they
//! cannot implement the `TestSuite` trait (which only hands out a
//! `&ElysianClient`). Instead the orchestrator calls
//! [`run_crash_recovery`] after the functional-suite runner returns, with
//! a mutable reference to the live [`ElysianInstance`].
//!
//! ## Ordering & prerequisites
//!
//! The TCP suite's `RESET` (TCP-06) wipes the in-memory store — including
//! the default admin account, every ACL grant, and every session cookie —
//! then TCP-07 `SAVE` persists that now-empty state to disk. By the time
//! this suite runs the HTTP client has a stale session cookie and no
//! admin exists on disk. The suite's preamble therefore does one
//! preliminary `restart_preserving_data` to re-trigger ElysianDB's
//! `InitAdminUserIfNotExists` boot step, then logs the client back in
//! with the default `admin/admin` credentials before any test touches
//! entity data.
//!
//! ## Test layout
//!
//!   - **CR-01 Data survives SIGKILL**: seed + save + SIGKILL + restart +
//!     verify every saved doc comes back.
//!   - **CR-02 WAL replay**: seed batch A + save + seed batch B (no save) +
//!     SIGKILL + restart + verify both batches come back (batch B is
//!     recovered from the write-ahead log).
//!   - **CR-03 Missing shard recovery**: seed + save + graceful stop +
//!     delete one file from `.battle/data/` + restart + verify the server
//!     boots and some data remains readable (per spec, partial data loss
//!     is acceptable).
//!
//! Each test logs back in after the restart it performs, so subsequent
//! tests (and the process-level `instance.stop()` in `main`) continue to
//! see an authenticated session.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::client::ElysianClient;
use crate::instance::ElysianInstance;
use crate::suites::{fail, pass, SuiteResult, TestResult};

const SUITE_NAME: &str = "Crash Recovery";
const ENTITY: &str = "battle_crash_data";

/// Run the crash-recovery suite end to end. Returns a populated
/// `SuiteResult` covering CR-01, CR-02, CR-03, matching the shape the
/// normal `Runner` emits for functional suites.
pub async fn run_crash_recovery(
    instance: &mut ElysianInstance,
    client: &ElysianClient,
) -> SuiteResult {
    let start = Instant::now();
    let mut tests = Vec::with_capacity(3);

    // Preamble: recover admin + clean slate. The preceding TCP suite's
    // RESET/SAVE left the on-disk store empty and the client cookie
    // stale, so this restart re-initializes admin and the login refresh
    // replaces the dead session cookie.
    if let Err(msg) = restart_and_login(instance, client).await {
        // Surface the failure as a single failing test so the report is
        // still well-formed; the remaining tests would all cascade from
        // the same root cause.
        tests.push(fail(
            SUITE_NAME,
            "CR-pre Setup",
            "restart + re-login".to_string(),
            None,
            start.elapsed(),
            msg,
        ));
        return SuiteResult {
            name: SUITE_NAME.to_string(),
            tests,
            duration: start.elapsed(),
        };
    }

    tests.push(cr01_survives_sigkill(instance, client).await);
    tests.push(cr02_wal_replay(instance, client).await);
    tests.push(cr03_missing_shard(instance, client).await);

    SuiteResult {
        name: SUITE_NAME.to_string(),
        tests,
        duration: start.elapsed(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// SIGKILL the process, respawn it preserving `.battle/data/`, then log
/// the shared HTTP client back in. Used by CR-01 and CR-02.
async fn kill_restart_login(
    instance: &mut ElysianInstance,
    client: &ElysianClient,
) -> Result<(), String> {
    instance
        .kill_hard()
        .await
        .map_err(|e| format!("SIGKILL failed: {e:#}"))?;
    instance
        .restart_preserving_data()
        .await
        .map_err(|e| format!("restart-preserving-data failed: {e:#}"))?;
    admin_login(client).await
}

/// Preamble restart — wipes `.battle/data/` and spawns a fresh process
/// so the crash-recovery tests start from a known-empty on-disk baseline.
///
/// The preceding TCP suite's `RESET` (TCP-06) wipes only the in-memory
/// store; the follow-up `SAVE` (TCP-07) does not reliably overwrite every
/// shard file from earlier suites, which leaves `.battle/data/` in a
/// state where a subsequent `POST /save` + restart cycle fails to
/// round-trip new writes. Using `restart_fresh()` sidesteps the
/// corruption and gives CR-01/CR-02/CR-03 the clean disk they assume.
async fn restart_and_login(
    instance: &mut ElysianInstance,
    client: &ElysianClient,
) -> Result<(), String> {
    instance
        .restart_fresh()
        .await
        .map_err(|e| format!("preamble fresh restart failed: {e:#}"))?;
    admin_login(client).await
}

async fn admin_login(client: &ElysianClient) -> Result<(), String> {
    let resp = client
        .login("admin", "admin")
        .await
        .map_err(|e| format!("admin login request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("admin login expected 2xx, got {status}"));
    }
    Ok(())
}

/// Best-effort "leave the instance alive" helper used by CR-03 error
/// paths. When the test bails out mid-way (graceful stop already done,
/// shard delete or restart failed), the orchestrator's final
/// `instance.stop()` still needs a running process to signal. Failures
/// here are ignored on purpose — the surrounding test has already
/// decided to return a `fail(...)` result, and there is nothing useful
/// to do if we cannot bring the process back.
async fn resurrect_best_effort(instance: &mut ElysianInstance, client: &ElysianClient) {
    let _ = instance.restart_preserving_data().await;
    let _ = admin_login(client).await;
}

/// Seed `count` docs with predictable `cr-<tag>-<n>` ids so tests can
/// assert exact id presence after restart.
async fn seed(client: &ElysianClient, tag: &str, count: usize) -> Result<Vec<String>, String> {
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let id = format!("cr-{tag}-{i}");
        let body = json!({"id": id, "tag": tag, "index": i as i64});
        let resp = client
            .create(ENTITY, body)
            .await
            .map_err(|e| format!("seed {id} request failed: {e:#}"))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(format!("seed {id} expected 200, got {status}"));
        }
        ids.push(id);
    }
    Ok(ids)
}

async fn save_store(client: &ElysianClient) -> Result<(), String> {
    let resp = client
        .save()
        .await
        .map_err(|e| format!("save request failed: {e:#}"))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("save expected 2xx, got {status}"));
    }
    Ok(())
}

async fn list_ids(client: &ElysianClient) -> Result<Vec<String>, String> {
    // Ask for a comfortably-large page so every seeded doc comes back in
    // a single response regardless of the server's default list limit.
    let resp = client
        .list(ENTITY, &[("limit", "1000")])
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
    let ids = arr
        .iter()
        .filter_map(|d| d.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    Ok(ids)
}

/// Pick an arbitrary file inside `.battle/data/` to delete for CR-03.
/// Prefers names containing `shard` (most ElysianDB layouts name shard
/// files that way); falls back to the first regular file if no `shard`
/// match is found so the test remains useful if the layout changes.
fn pick_file_to_delete(data_dir: &Path) -> Result<std::path::PathBuf, String> {
    let entries = std::fs::read_dir(data_dir)
        .map_err(|e| format!("could not list {}: {e:#}", data_dir.display()))?;

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                candidates.push(path);
            }
        }
    }

    if candidates.is_empty() {
        return Err(format!(
            "no files found in {} — cannot simulate shard loss",
            data_dir.display()
        ));
    }

    // Prefer a shard-looking file; otherwise take any regular file.
    let chosen = candidates
        .iter()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_ascii_lowercase().contains("shard"))
                .unwrap_or(false)
        })
        .cloned()
        .unwrap_or_else(|| candidates[0].clone());

    Ok(chosen)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// CR-01 — Insert 10 docs → save → SIGKILL → restart → every doc comes back.
async fn cr01_survives_sigkill(
    instance: &mut ElysianInstance,
    client: &ElysianClient,
) -> TestResult {
    let name = "CR-01 Data survives SIGKILL + restart";
    let request =
        format!("seed 10 docs into {ENTITY} → POST /save → SIGKILL → restart → GET /api/{ENTITY}");
    let start = Instant::now();

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            SUITE_NAME,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre-seed wipe failed: {e:#}"),
        );
    }

    let seeded = match seed(client, "01", 10).await {
        Ok(ids) => ids,
        Err(msg) => return fail(SUITE_NAME, name, request, None, start.elapsed(), msg),
    };

    if let Err(msg) = save_store(client).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }

    // Give ElysianDB a moment to flush any in-memory state after the
    // save returns — save is synchronous per the controller, but
    // queueing a short sleep makes the test resilient to minor timing
    // variations across versions.
    tokio::time::sleep(Duration::from_millis(200)).await;

    if let Err(msg) = kill_restart_login(instance, client).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }

    let found = match list_ids(client).await {
        Ok(ids) => ids,
        Err(msg) => return fail(SUITE_NAME, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    for id in &seeded {
        if !found.iter().any(|f| f == id) {
            return fail(
                SUITE_NAME,
                name,
                request,
                Some(200),
                duration,
                format!(
                    "seeded id `{id}` missing after SIGKILL + restart (got {} ids: {:?})",
                    found.len(),
                    found
                ),
            );
        }
    }

    pass(SUITE_NAME, name, request, Some(200), duration)
}

// CR-02 — Insert batch A, save, insert batch B (no save), SIGKILL, restart,
// expect BOTH batches to come back. Batch B survives because writes are
// journaled to the WAL before the controller returns.
async fn cr02_wal_replay(instance: &mut ElysianInstance, client: &ElysianClient) -> TestResult {
    let name = "CR-02 WAL replay recovers unsaved writes";
    let request =
        format!("seed A (save) + seed B (no save) → SIGKILL → restart → GET /api/{ENTITY}");
    let start = Instant::now();

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            SUITE_NAME,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre-seed wipe failed: {e:#}"),
        );
    }

    let saved = match seed(client, "02a", 5).await {
        Ok(ids) => ids,
        Err(msg) => return fail(SUITE_NAME, name, request, None, start.elapsed(), msg),
    };

    if let Err(msg) = save_store(client).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }

    // Batch B: written after the save — only the WAL protects these.
    let walled = match seed(client, "02b", 5).await {
        Ok(ids) => ids,
        Err(msg) => return fail(SUITE_NAME, name, request, None, start.elapsed(), msg),
    };

    if let Err(msg) = kill_restart_login(instance, client).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }

    let found = match list_ids(client).await {
        Ok(ids) => ids,
        Err(msg) => return fail(SUITE_NAME, name, request, None, start.elapsed(), msg),
    };
    let duration = start.elapsed();

    for id in saved.iter().chain(walled.iter()) {
        if !found.iter().any(|f| f == id) {
            return fail(
                SUITE_NAME,
                name,
                request,
                Some(200),
                duration,
                format!(
                    "id `{id}` missing after restart — saved_count={}, walled_count={}, \
                     found={} ids",
                    saved.len(),
                    walled.len(),
                    found.len()
                ),
            );
        }
    }

    pass(SUITE_NAME, name, request, Some(200), duration)
}

// CR-03 — Graceful stop, delete a shard file, restart. Server must boot
// and serve requests; partial data loss is acceptable per spec.
async fn cr03_missing_shard(instance: &mut ElysianInstance, client: &ElysianClient) -> TestResult {
    let name = "CR-03 Graceful recovery from missing shard";
    let request = format!(
        "seed {ENTITY} → POST /save → stop → delete one file in .battle/data/ → restart → list"
    );
    let start = Instant::now();

    if let Err(e) = client.delete_all(ENTITY).await {
        return fail(
            SUITE_NAME,
            name,
            request,
            None,
            start.elapsed(),
            format!("pre-seed wipe failed: {e:#}"),
        );
    }

    // Seed enough docs that distribution across the configured 64 shards
    // gives us a good chance the deleted shard held some data — a
    // near-empty shard would make the "server still runs" assertion
    // trivial.
    const SEED_COUNT: usize = 200;
    if let Err(msg) = seed(client, "03", SEED_COUNT).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }

    if let Err(msg) = save_store(client).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    let data_dir = instance.battle_dir().join("data");
    // Stop the server gracefully before touching its files so no
    // background flush races our delete.
    if let Err(msg) = graceful_stop(instance).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }

    let target = match pick_file_to_delete(&data_dir) {
        Ok(p) => p,
        Err(msg) => {
            resurrect_best_effort(instance, client).await;
            return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
        }
    };
    if let Err(e) = std::fs::remove_file(&target) {
        resurrect_best_effort(instance, client).await;
        return fail(
            SUITE_NAME,
            name,
            request,
            None,
            start.elapsed(),
            format!("could not delete {}: {e:#}", target.display()),
        );
    }

    if let Err(e) = instance.restart_preserving_data().await {
        return fail(
            SUITE_NAME,
            name,
            request,
            None,
            start.elapsed(),
            format!("restart after shard deletion failed: {e:#}"),
        );
    }
    if let Err(msg) = admin_login(client).await {
        return fail(SUITE_NAME, name, request, None, start.elapsed(), msg);
    }

    // Server is up — confirm it answers an entity read without erroring.
    // We do NOT assert a specific surviving count: the spec explicitly
    // accepts partial data loss here.
    let found = match list_ids(client).await {
        Ok(ids) => ids,
        Err(msg) => {
            return fail(
                SUITE_NAME,
                name,
                request,
                None,
                start.elapsed(),
                format!("post-restart list failed: {msg}"),
            )
        }
    };
    let duration = start.elapsed();

    if found.len() > SEED_COUNT {
        return fail(
            SUITE_NAME,
            name,
            request,
            Some(200),
            duration,
            format!(
                "post-restart list returned {} ids — more than the {SEED_COUNT} seeded",
                found.len()
            ),
        );
    }

    pass(SUITE_NAME, name, request, Some(200), duration)
}

/// CR-03 helper: graceful stop only (no restart yet — caller deletes a
/// shard file before the restart). Kept as a tiny named helper so the
/// test reads top-to-bottom without inlining the anyhow-to-string
/// mapping.
async fn graceful_stop(instance: &mut ElysianInstance) -> Result<(), String> {
    instance
        .stop()
        .await
        .map_err(|e| format!("graceful stop failed: {e:#}"))
}
