//! Suite 17 — Performance Benchmarks (8 scenarios, P-01..P-08).
//!
//! Metrics-only suite: measures latency percentiles (p50, p95, p99) and
//! request throughput. Never produces pass/fail `TestResult`s — returns
//! `Vec<PerformanceResult>` which the orchestrator folds into
//! `BattleReport.performance`.
//!
//! Like `crash_recovery`, this suite runs outside `Runner` because its
//! result shape does not fit the `TestSuite` trait. `main.rs` drives it
//! after the functional runner + crash-recovery complete, so it always
//! runs last against a clean, stable instance.
//!
//! ## Percentile computation
//!
//! ```text
//! sorted = sort(durations)
//! p50 = sorted[len * 0.50]
//! p95 = sorted[len * 0.95]
//! p99 = sorted[len * 0.99]
//! throughput = iterations / total_elapsed_seconds
//! ```
//!
//! ## Data dependency between scenarios
//!
//! P-01 seeds 200 documents and captures the first created id; the
//! combined output of P-01 (200 docs) + P-02 (50 × 100 = 5000 docs)
//! gives downstream scenarios a populated entity to read from without a
//! dedicated seed phase. P-03 reuses the captured id, P-04..P-07 target
//! the live entity, P-08 is KV-only and independent.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tracing::warn;

use crate::client::ElysianClient;
use crate::suites::PerformanceResult;

pub const SUITE_NAME: &str = "Performance";
const ENTITY: &str = "battle_perf_items";
const KV_KEY: &str = "battle_perf_kv_key";

/// Run the full performance benchmark suite end-to-end.
///
/// Assumes the caller has already logged the client in as admin. Cleans
/// up its own entity + KV key before and after so repeated runs stay
/// deterministic.
pub async fn run_performance(client: &ElysianClient) -> Vec<PerformanceResult> {
    let mut results = Vec::with_capacity(8);

    let _ = client.delete_all(ENTITY).await;
    let _ = client.kv_delete(KV_KEY).await;

    let (p01, sample_id) = p01_single_create(client).await;
    results.push(p01);
    results.push(p02_batch_create(client).await);

    match sample_id {
        Some(id) => results.push(p03_single_get(client, &id).await),
        None => warn!("P-03 skipped: no sample id captured from P-01"),
    }

    results.push(p04_list_1000(client).await);
    results.push(p05_filtered_query(client).await);
    results.push(p06_sorted_query(client).await);
    results.push(p07_concurrent_reads(client).await);
    results.push(p08_kv_cycle(client).await);

    let _ = client.delete_all(ENTITY).await;
    let _ = client.kv_delete(KV_KEY).await;

    results
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

async fn p01_single_create(client: &ElysianClient) -> (PerformanceResult, Option<String>) {
    const ITER: u64 = 200;
    let mut durations = Vec::with_capacity(ITER as usize);
    let mut sample_id: Option<String> = None;
    let total_start = Instant::now();

    for i in 0..ITER {
        let start = Instant::now();
        let resp = client.create(ENTITY, json!({"value": i as i64})).await;
        let elapsed = start.elapsed();
        match resp {
            Ok(r) if r.status().is_success() => {
                durations.push(elapsed);
                if sample_id.is_none() {
                    if let Ok(body) = r.json::<Value>().await {
                        if let Some(id) = body.get("id").and_then(|v| v.as_str()) {
                            sample_id = Some(id.to_string());
                        }
                    }
                }
            }
            Ok(r) => warn!("P-01 create iteration {i} got status {}", r.status()),
            Err(e) => warn!("P-01 create iteration {i} failed: {e:#}"),
        }
    }

    let total = total_start.elapsed();
    (
        compute_percentiles("P-01 Single create", durations, ITER, total),
        sample_id,
    )
}

async fn p02_batch_create(client: &ElysianClient) -> PerformanceResult {
    const ITER: u64 = 50;
    const BATCH: usize = 100;
    let mut durations = Vec::with_capacity(ITER as usize);
    let total_start = Instant::now();

    for i in 0..ITER {
        let batch: Vec<Value> = (0..BATCH)
            .map(|j| json!({"value": (i as i64) * 1000 + j as i64, "batch": i as i64}))
            .collect();
        let payload = Value::Array(batch);

        let start = Instant::now();
        let resp = client.create(ENTITY, payload).await;
        let elapsed = start.elapsed();
        match resp {
            Ok(r) if r.status().is_success() => durations.push(elapsed),
            Ok(r) => warn!("P-02 batch iteration {i} got status {}", r.status()),
            Err(e) => warn!("P-02 batch iteration {i} failed: {e:#}"),
        }
    }

    let total = total_start.elapsed();
    compute_percentiles("P-02 Batch create (100 docs)", durations, ITER, total)
}

async fn p03_single_get(client: &ElysianClient, id: &str) -> PerformanceResult {
    const ITER: u64 = 200;
    let mut durations = Vec::with_capacity(ITER as usize);
    let total_start = Instant::now();

    for i in 0..ITER {
        let start = Instant::now();
        let resp = client.get(ENTITY, id).await;
        let elapsed = start.elapsed();
        match resp {
            Ok(r) if r.status().is_success() => durations.push(elapsed),
            Ok(r) => warn!("P-03 get iteration {i} got status {}", r.status()),
            Err(e) => warn!("P-03 get iteration {i} failed: {e:#}"),
        }
    }

    let total = total_start.elapsed();
    compute_percentiles("P-03 Single get by ID", durations, ITER, total)
}

async fn p04_list_1000(client: &ElysianClient) -> PerformanceResult {
    const ITER: u64 = 50;
    let mut durations = Vec::with_capacity(ITER as usize);
    let total_start = Instant::now();

    for i in 0..ITER {
        let start = Instant::now();
        let resp = client.list(ENTITY, &[("limit", "1000")]).await;
        let elapsed = start.elapsed();
        match resp {
            Ok(r) if r.status().is_success() => durations.push(elapsed),
            Ok(r) => warn!("P-04 list iteration {i} got status {}", r.status()),
            Err(e) => warn!("P-04 list iteration {i} failed: {e:#}"),
        }
    }

    let total = total_start.elapsed();
    compute_percentiles("P-04 List 1000 docs", durations, ITER, total)
}

async fn p05_filtered_query(client: &ElysianClient) -> PerformanceResult {
    const ITER: u64 = 200;
    let mut durations = Vec::with_capacity(ITER as usize);
    let total_start = Instant::now();

    // Filter values must be JSON strings (see query.rs header). Targets a
    // value seeded by P-01 (value: 42) so the filter has a deterministic
    // non-empty match in every iteration.
    let body = json!({
        "entity": ENTITY,
        "filters": {"and": [{"value": {"eq": "42"}}]}
    });

    for i in 0..ITER {
        let start = Instant::now();
        let resp = client.query(body.clone()).await;
        let elapsed = start.elapsed();
        match resp {
            Ok(r) if r.status().is_success() => durations.push(elapsed),
            Ok(r) => warn!("P-05 query iteration {i} got status {}", r.status()),
            Err(e) => warn!("P-05 query iteration {i} failed: {e:#}"),
        }
    }

    let total = total_start.elapsed();
    compute_percentiles("P-05 Filtered query", durations, ITER, total)
}

async fn p06_sorted_query(client: &ElysianClient) -> PerformanceResult {
    const ITER: u64 = 100;
    let mut durations = Vec::with_capacity(ITER as usize);
    let total_start = Instant::now();

    for i in 0..ITER {
        let start = Instant::now();
        let resp = client
            .list(ENTITY, &[("sort[value]", "asc"), ("limit", "100")])
            .await;
        let elapsed = start.elapsed();
        match resp {
            Ok(r) if r.status().is_success() => durations.push(elapsed),
            Ok(r) => warn!("P-06 sorted iteration {i} got status {}", r.status()),
            Err(e) => warn!("P-06 sorted iteration {i} failed: {e:#}"),
        }
    }

    let total = total_start.elapsed();
    compute_percentiles("P-06 Sorted query", durations, ITER, total)
}

async fn p07_concurrent_reads(client: &ElysianClient) -> PerformanceResult {
    const BATCHES: u64 = 100;
    const PARALLEL: usize = 10;
    const TOTAL: u64 = BATCHES * PARALLEL as u64;

    let mut durations = Vec::with_capacity(TOTAL as usize);
    let total_start = Instant::now();

    for b in 0..BATCHES {
        let mut handles = Vec::with_capacity(PARALLEL);
        for _ in 0..PARALLEL {
            let c = client.clone();
            handles.push(tokio::spawn(async move {
                let start = Instant::now();
                let resp = c.list(ENTITY, &[("limit", "10")]).await;
                (start.elapsed(), resp.map(|r| r.status().is_success()))
            }));
        }

        for h in handles {
            match h.await {
                Ok((elapsed, Ok(true))) => durations.push(elapsed),
                Ok((_, Ok(false))) => warn!("P-07 batch {b} a request returned non-success"),
                Ok((_, Err(e))) => warn!("P-07 batch {b} request failed: {e:#}"),
                Err(e) => warn!("P-07 batch {b} join error: {e:#}"),
            }
        }
    }

    let total = total_start.elapsed();
    compute_percentiles(
        "P-07 Concurrent reads (10 parallel)",
        durations,
        TOTAL,
        total,
    )
}

async fn p08_kv_cycle(client: &ElysianClient) -> PerformanceResult {
    const ITER: u64 = 500;
    let mut durations = Vec::with_capacity(ITER as usize);
    let total_start = Instant::now();

    for i in 0..ITER {
        let value = format!("v{i}");
        let start = Instant::now();
        let set = client.kv_set(KV_KEY, &value, None).await;
        let get = client.kv_get(KV_KEY).await;
        let elapsed = start.elapsed();

        let set_ok = matches!(&set, Ok(r) if r.status().is_success());
        let get_ok = matches!(&get, Ok(r) if r.status().is_success());
        if set_ok && get_ok {
            durations.push(elapsed);
        } else {
            warn!("P-08 kv cycle iteration {i} failed (set_ok={set_ok}, get_ok={get_ok})");
        }
    }

    let total = total_start.elapsed();
    compute_percentiles("P-08 KV set/get cycle", durations, ITER, total)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `PerformanceResult` from a list of per-request durations, the
/// number of iterations attempted, and the wall-clock total for
/// throughput. Zero-pads percentiles and throughput when no samples
/// succeeded so downstream reporting stays well-formed.
fn compute_percentiles(
    scenario: &str,
    mut durations: Vec<Duration>,
    iterations: u64,
    total: Duration,
) -> PerformanceResult {
    durations.sort();
    let len = durations.len();

    let (p50, p95, p99) = if len == 0 {
        (Duration::ZERO, Duration::ZERO, Duration::ZERO)
    } else {
        (
            durations[percentile_index(len, 0.50)],
            durations[percentile_index(len, 0.95)],
            durations[percentile_index(len, 0.99)],
        )
    };

    let total_secs = total.as_secs_f64();
    let throughput = if total_secs > 0.0 {
        iterations as f64 / total_secs
    } else {
        0.0
    };

    PerformanceResult {
        scenario: scenario.to_string(),
        iterations,
        p50,
        p95,
        p99,
        throughput,
    }
}

/// Index into a `len`-sized sorted slice for the given percentile `p`
/// (0.0..=1.0). Clamps to `len - 1` so `p == 1.0` and rounding overshoot
/// stay in range; `len == 0` must be handled by the caller.
fn percentile_index(len: usize, p: f64) -> usize {
    debug_assert!(len > 0, "percentile_index requires len > 0");
    let idx = (len as f64 * p) as usize;
    idx.min(len - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_index_middle() {
        assert_eq!(percentile_index(200, 0.50), 100);
        assert_eq!(percentile_index(200, 0.95), 190);
        assert_eq!(percentile_index(200, 0.99), 198);
    }

    #[test]
    fn percentile_index_clamps_at_upper_bound() {
        assert_eq!(percentile_index(100, 1.0), 99);
    }

    #[test]
    fn percentile_index_singleton() {
        assert_eq!(percentile_index(1, 0.50), 0);
        assert_eq!(percentile_index(1, 0.99), 0);
    }

    #[test]
    fn compute_percentiles_empty_samples_returns_zeros() {
        let r = compute_percentiles("x", vec![], 10, Duration::from_secs(1));
        assert_eq!(r.iterations, 10);
        assert_eq!(r.p50, Duration::ZERO);
        assert_eq!(r.p95, Duration::ZERO);
        assert_eq!(r.p99, Duration::ZERO);
        assert_eq!(r.throughput, 10.0);
    }

    #[test]
    fn compute_percentiles_sorts_before_indexing() {
        let samples = vec![
            Duration::from_millis(50),
            Duration::from_millis(10),
            Duration::from_millis(30),
            Duration::from_millis(40),
            Duration::from_millis(20),
        ];
        let r = compute_percentiles("x", samples, 5, Duration::from_secs(1));
        assert_eq!(r.p50, Duration::from_millis(30));
        assert_eq!(r.p95, Duration::from_millis(50));
        assert_eq!(r.p99, Duration::from_millis(50));
    }

    #[test]
    fn compute_percentiles_throughput_on_zero_elapsed() {
        let r = compute_percentiles("x", vec![Duration::from_millis(1)], 10, Duration::ZERO);
        assert_eq!(r.throughput, 0.0);
    }

    #[test]
    fn compute_percentiles_throughput_ratio() {
        let r = compute_percentiles(
            "x",
            vec![Duration::from_millis(1); 100],
            100,
            Duration::from_secs(2),
        );
        assert!((r.throughput - 50.0).abs() < 1e-9);
    }
}
