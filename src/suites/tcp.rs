//! Suite 11 — TCP Protocol (8 tests, TCP-01..TCP-08).
//!
//! Exercises ElysianDB's text-line TCP protocol
//! (`internal/transport/tcp/tcp_routing/route.go`). Each test opens a
//! fresh `ElysianTcpClient` so connection state doesn't leak between
//! assertions.
//!
//! ## Response formats
//!
//!   - `PING` → `PONG`
//!   - `SET key value` / `SET TTL=N key value` → `OK` (or `ERR`)
//!   - `GET key` → `key=value` on hit, `key=not found` on miss
//!   - `MGET k1 k2 ...` → one line per key, joined by `\n` and terminated
//!     by the server's trailing `\n` in `boot/tcp.go:handleConnection`.
//!     **Asymmetric with single GET**: on a hit each line is the raw value
//!     only (no `key=` prefix — see `handler/multi_get.go:HandleMGETSingleKey`,
//!     which writes `(*results)[i] = data`), while on a miss it's
//!     `key=not found`. The client reads one line per requested key.
//!   - `DEL key` → `Deleted N`
//!   - `RESET` → `OK` (destructive — see the TCP-06 placement note below)
//!   - `SAVE` → `OK`
//!
//! ## TCP-06 RESET — destructive, ordering-dependent
//!
//! `RESET` invokes `storage.ResetStore()`, which wipes the entire
//! in-memory KV store — including every `_elysiandb_core_user:*` record
//! (so the default `admin` account disappears), every ACL grant, and
//! every active session cookie. `security.InitAdminUserIfNotExists`
//! only runs at process boot (`elysiandb.go:main`), so nothing inside
//! the live process restores admin after a RESET.
//!
//! We can still exercise the command because:
//!
//!   1. The TCP protocol layer has no auth middleware
//!      (`boot/tcp.go:handleConnection` dispatches straight to
//!      `RouteLine`), so every TCP test remaining in the suite works
//!      on a wiped store.
//!   2. `TcpSuite` is registered LAST in `all_suites()` among the
//!      auth-dependent functional suites, and the runner does no
//!      admin-gated work after the final suite returns — `instance.stop`
//!      just sends a signal to the process.
//!   3. The Crash Recovery suite (future — ticket #12+) will restart the
//!      ElysianDB process, which re-runs `InitAdminUserIfNotExists` and
//!      re-creates the default admin from scratch.
//!
//! **If you add a new auth-gated suite, register it BEFORE `TcpSuite`
//! in `all_suites()`** — otherwise its `setup`/cleanup path will hit an
//! empty store. The suite-count unit test pins `TcpSuite` at the last
//! index to catch accidental reordering.

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;

use crate::client::ElysianClient;
use crate::suites::{fail, pass, TestResult, TestSuite};
use crate::tcp_client::ElysianTcpClient;

/// Every key this suite sets over TCP. Used by setup/teardown for
/// idempotent cleanup via `DEL`. `battle_tcp_nonexistent` is listed
/// as a defensive measure only — TCP-08 never writes it, but DEL on a
/// missing key is a no-op so this stays safe.
const TCP_KEYS: &[&str] = &[
    "battle_tcp_k1",
    "battle_tcp_ttl",
    "battle_tcp_m1",
    "battle_tcp_m2",
    "battle_tcp_m3",
    "battle_tcp_reset_sentinel",
    "battle_tcp_save",
    "battle_tcp_nonexistent",
];

pub struct TcpSuite {
    tcp_port: u16,
}

impl TcpSuite {
    pub fn new(tcp_port: u16) -> Self {
        // Port `0` never lands in runtime — the harness picks a real
        // port via `port::find_available_ports()` before constructing
        // the suite. `debug_assert!` catches accidental `all_suites(0)`
        // callers in debug builds while staying out of the release path.
        // The `all_suites` unit test passes a non-zero placeholder to
        // respect this invariant.
        debug_assert!(
            tcp_port != 0,
            "TcpSuite built with port 0 — did the runner forget to pass a live port?"
        );
        Self { tcp_port }
    }
}

#[async_trait]
impl TestSuite for TcpSuite {
    fn name(&self) -> &'static str {
        "TCP Protocol"
    }

    fn description(&self) -> &'static str {
        "Validates TCP text protocol: PING/PONG, SET/GET, SET TTL + expiration, MGET, DEL, SAVE, non-existent key"
    }

    async fn setup(&self, _client: &ElysianClient) -> Result<()> {
        cleanup_keys(self.tcp_port).await;
        Ok(())
    }

    async fn run(&self, _client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let port = self.tcp_port;
        let mut results = Vec::with_capacity(8);

        results.push(tcp01_ping_pong(&suite, port).await);
        results.push(tcp02_set_and_get(&suite, port).await);
        results.push(tcp03_set_with_ttl(&suite, port).await);
        results.push(tcp04_mget(&suite, port).await);
        results.push(tcp05_del(&suite, port).await);
        results.push(tcp06_reset(&suite, port).await);
        results.push(tcp07_save(&suite, port).await);
        results.push(tcp08_get_non_existent(&suite, port).await);

        results
    }

    async fn teardown(&self, _client: &ElysianClient) -> Result<()> {
        cleanup_keys(self.tcp_port).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Best-effort delete every known test key over TCP. A single `DEL` per
/// key is cheap and tolerates missing keys (the server just reports
/// `Deleted 0`).
async fn cleanup_keys(port: u16) {
    let Ok(mut tcp) = ElysianTcpClient::connect(port).await else {
        return;
    };
    for key in TCP_KEYS {
        let _ = tcp.del(key).await;
    }
}

/// `GET key` returns `key=value` for hits and `key=not found` for
/// misses. Returns `Some(value)` on hit, `None` on miss.
fn parse_get_response(response: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    let rest = response.strip_prefix(&prefix)?;
    if rest == "not found" {
        None
    } else {
        Some(rest.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// TCP-01 — PING returns PONG.
async fn tcp01_ping_pong(suite: &str, port: u16) -> TestResult {
    let name = "TCP-01 PING";
    let request = "PING".to_string();
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };
    let resp = match tcp.ping().await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("PING failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if resp == "PONG" {
        pass(suite, name, request, None, duration)
    } else {
        fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("expected \"PONG\", got \"{resp}\""),
        )
    }
}

// TCP-02 — SET then GET returns the same value.
async fn tcp02_set_and_get(suite: &str, port: u16) -> TestResult {
    let name = "TCP-02 SET and GET";
    let key = "battle_tcp_k1";
    let value = "value1";
    let request = format!("SET {key} {value} + GET {key}");
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };
    let set_resp = match tcp.set(key, value).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("SET failed: {e:#}"),
            )
        }
    };
    if set_resp != "OK" {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("SET expected \"OK\", got \"{set_resp}\""),
        );
    }

    let get_resp = match tcp.get(key).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("GET failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    match parse_get_response(&get_resp, key) {
        Some(actual) if actual == value => pass(suite, name, request, None, duration),
        Some(actual) => fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("expected value=\"{value}\", got \"{actual}\""),
        ),
        None => fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("expected {key}={value}, got \"{get_resp}\""),
        ),
    }
}

// TCP-03 — SET TTL=2, wait 3s, GET reports expiration.
async fn tcp03_set_with_ttl(suite: &str, port: u16) -> TestResult {
    let name = "TCP-03 SET with TTL";
    let key = "battle_tcp_ttl";
    let request = format!("SET TTL=2 {key} val + wait 3s + GET {key}");
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };
    let set_resp = match tcp.set_ttl(key, "val", 2).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("SET TTL failed: {e:#}"),
            )
        }
    };
    if set_resp != "OK" {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("SET TTL expected \"OK\", got \"{set_resp}\""),
        );
    }

    tokio::time::sleep(Duration::from_secs(3)).await;

    let get_resp = match tcp.get(key).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("GET failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if parse_get_response(&get_resp, key).is_none() {
        pass(suite, name, request, None, duration)
    } else {
        fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("expected expired ({key}=not found), got \"{get_resp}\""),
        )
    }
}

// TCP-04 — MGET returns one line per requested key.
async fn tcp04_mget(suite: &str, port: u16) -> TestResult {
    let name = "TCP-04 MGET";
    let keys = ["battle_tcp_m1", "battle_tcp_m2", "battle_tcp_m3"];
    let values = ["v1", "v2", "v3"];
    let request = format!("SET 3 keys + MGET {}", keys.join(" "));
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };

    for (key, value) in keys.iter().zip(values.iter()) {
        let resp = match tcp.set(key, value).await {
            Ok(r) => r,
            Err(e) => {
                return fail(
                    suite,
                    name,
                    request,
                    None,
                    start.elapsed(),
                    format!("SET {key} failed: {e:#}"),
                )
            }
        };
        if resp != "OK" {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("SET {key} expected \"OK\", got \"{resp}\""),
            );
        }
    }

    let key_refs: Vec<&str> = keys.to_vec();
    let lines = match tcp.mget(&key_refs).await {
        Ok(ls) => ls,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("MGET failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if lines.len() != keys.len() {
        return fail(
            suite,
            name,
            request,
            None,
            duration,
            format!(
                "expected {} lines, got {}: {lines:?}",
                keys.len(),
                lines.len()
            ),
        );
    }

    // MGET hits return raw values (no `key=` prefix) while misses return
    // `key=not found` — see suite-level docs. All three keys were just
    // SET, so every line must equal the corresponding value verbatim.
    for (idx, (key, value)) in keys.iter().zip(values.iter()).enumerate() {
        if lines[idx] != *value {
            return fail(
                suite,
                name,
                request,
                None,
                duration,
                format!(
                    "line {idx} for {key}: expected \"{value}\", got \"{}\"",
                    lines[idx]
                ),
            );
        }
    }

    pass(suite, name, request, None, duration)
}

// TCP-05 — DEL reports how many keys were removed.
async fn tcp05_del(suite: &str, port: u16) -> TestResult {
    let name = "TCP-05 DEL";
    let key = "battle_tcp_k1";
    let request = format!("SET {key} v + DEL {key} + GET {key}");
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };

    // Seed the key first so DEL has something to report.
    let set_resp = match tcp.set(key, "to-delete").await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("seed SET failed: {e:#}"),
            )
        }
    };
    if set_resp != "OK" {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("seed SET expected \"OK\", got \"{set_resp}\""),
        );
    }

    let del_resp = match tcp.del(key).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("DEL failed: {e:#}"),
            )
        }
    };
    if !del_resp.starts_with("Deleted ") {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("DEL expected \"Deleted N\", got \"{del_resp}\""),
        );
    }

    // Confirm the key is gone on a subsequent GET.
    let get_resp = match tcp.get(key).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("follow-up GET failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if parse_get_response(&get_resp, key).is_none() {
        pass(suite, name, request, None, duration)
    } else {
        fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("key still present after DEL: \"{get_resp}\""),
        )
    }
}

// TCP-06 — RESET returns OK and actually clears the store.
//
// Seed a sentinel key, call RESET, verify it responds `OK`, then confirm
// the sentinel is gone. Safe to run here because TCP-06 is the last
// auth-sensitive thing the harness does (see suite-level docs for the
// ordering contract this relies on).
async fn tcp06_reset(suite: &str, port: u16) -> TestResult {
    let name = "TCP-06 RESET";
    let sentinel = "battle_tcp_reset_sentinel";
    let request = format!("SET {sentinel} + RESET + GET {sentinel}");
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };

    // Seed a sentinel so the "all keys cleared" assertion has something
    // concrete to disappear.
    let set_resp = match tcp.set(sentinel, "present").await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("seed SET failed: {e:#}"),
            )
        }
    };
    if set_resp != "OK" {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("seed SET expected \"OK\", got \"{set_resp}\""),
        );
    }

    let reset_resp = match tcp.reset().await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("RESET failed: {e:#}"),
            )
        }
    };
    if reset_resp != "OK" {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("RESET expected \"OK\", got \"{reset_resp}\""),
        );
    }

    let get_resp = match tcp.get(sentinel).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("post-RESET GET failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if parse_get_response(&get_resp, sentinel).is_none() {
        pass(suite, name, request, None, duration)
    } else {
        fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("sentinel survived RESET: \"{get_resp}\""),
        )
    }
}

// TCP-07 — SAVE returns OK.
async fn tcp07_save(suite: &str, port: u16) -> TestResult {
    let name = "TCP-07 SAVE";
    let request = "SET battle_tcp_save v + SAVE".to_string();
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };

    // Write at least one key so SAVE has something to flush. SAVE works
    // even on an empty store, but seeding makes the test intent clearer
    // and exercises the flush path end-to-end.
    let set_resp = match tcp.set("battle_tcp_save", "saved").await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("seed SET failed: {e:#}"),
            )
        }
    };
    if set_resp != "OK" {
        return fail(
            suite,
            name,
            request,
            None,
            start.elapsed(),
            format!("seed SET expected \"OK\", got \"{set_resp}\""),
        );
    }

    let resp = match tcp.save().await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("SAVE failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    if resp == "OK" {
        pass(suite, name, request, None, duration)
    } else {
        fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("expected \"OK\", got \"{resp}\""),
        )
    }
}

// TCP-08 — GET on a never-set key reports "not found".
async fn tcp08_get_non_existent(suite: &str, port: u16) -> TestResult {
    let name = "TCP-08 GET non-existent";
    let key = "battle_tcp_nonexistent";
    let request = format!("GET {key}");
    let start = Instant::now();

    let mut tcp = match ElysianTcpClient::connect(port).await {
        Ok(c) => c,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("TCP connect failed: {e:#}"),
            )
        }
    };
    let resp = match tcp.get(key).await {
        Ok(r) => r,
        Err(e) => {
            return fail(
                suite,
                name,
                request,
                None,
                start.elapsed(),
                format!("GET failed: {e:#}"),
            )
        }
    };
    let duration = start.elapsed();

    // "Empty or error" per the spec. ElysianDB v0.1.14 returns
    // `{key}=not found` which parse_get_response maps to None.
    if parse_get_response(&resp, key).is_none() || resp.is_empty() {
        pass(suite, name, request, None, duration)
    } else {
        fail(
            suite,
            name,
            request,
            None,
            duration,
            format!("expected missing ({key}=not found or empty), got \"{resp}\""),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_get_response_hit() {
        assert_eq!(
            parse_get_response("battle_tcp_k1=hello", "battle_tcp_k1"),
            Some("hello".to_string())
        );
    }

    #[test]
    fn parse_get_response_miss() {
        assert_eq!(
            parse_get_response("battle_tcp_k1=not found", "battle_tcp_k1"),
            None
        );
    }

    #[test]
    fn parse_get_response_wrong_key_returns_none() {
        assert_eq!(parse_get_response("other=hello", "battle_tcp_k1"), None);
    }

    #[test]
    fn parse_get_response_empty_value() {
        assert_eq!(
            parse_get_response("battle_tcp_k1=", "battle_tcp_k1"),
            Some(String::new())
        );
    }
}
