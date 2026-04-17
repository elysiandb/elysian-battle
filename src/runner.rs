use std::time::Instant;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::warn;

use crate::client::ElysianClient;
use crate::suites::{BattleReport, SuiteResult, TestStatus, TestSuite, BATTLE_ENTITIES};

/// Test orchestrator — discovers, filters, and executes suites sequentially.
pub struct Runner {
    suites: Vec<Box<dyn TestSuite>>,
    suite_filter: Option<Vec<String>>,
    /// Names of suites that are run by the caller *outside* `run()` but
    /// still participate in `--suite` filtering. Currently: `Crash
    /// Recovery`, which `main.rs` orchestrates directly because it needs
    /// `&mut ElysianInstance`. Used only to suppress the "no suites
    /// match filter" warning when the filter *does* match an external
    /// suite — the runner itself never touches these.
    external_suite_names: Vec<&'static str>,
}

impl Runner {
    pub fn new(suites: Vec<Box<dyn TestSuite>>, suite_filter: Option<Vec<String>>) -> Self {
        Self {
            suites,
            suite_filter,
            external_suite_names: Vec::new(),
        }
    }

    /// Register suite names that the caller runs directly (e.g. `Crash
    /// Recovery` driven from `main.rs`). These are used only so the
    /// filter-miss warning doesn't fire when the user invokes
    /// `--suite <external>` on its own.
    pub fn with_external_suites(mut self, names: Vec<&'static str>) -> Self {
        self.external_suite_names = names;
        self
    }

    /// Execute all matching suites sequentially with cleanup between each.
    ///
    /// Returns a complete `BattleReport` ready for reporting.
    pub async fn run(&self, client: &ElysianClient, elysiandb_version: &str) -> BattleReport {
        let total_start = Instant::now();
        let mut suite_results = Vec::new();

        let indices: Vec<usize> = self
            .suites
            .iter()
            .enumerate()
            .filter(|(_, s)| self.should_run(s.name()))
            .map(|(i, _)| i)
            .collect();

        if indices.is_empty() {
            if self.suites.is_empty() {
                println!(
                    "\n  {} No test suites registered (implementations pending)",
                    style("ℹ").cyan()
                );
            } else if let Some(ref filter) = self.suite_filter {
                // Don't warn when the filter matches a suite the caller
                // runs out-of-band — otherwise `--suite crash_recovery`
                // prints a misleading "nothing to run" right before the
                // external suite actually executes.
                let external_matches = self.external_suite_names.iter().any(|n| self.should_run(n));
                if !external_matches {
                    println!(
                        "\n  {} No suites match filter: {}",
                        style("⚠").yellow(),
                        filter.join(", ")
                    );
                }
            }
        } else {
            println!();
            let pb = ProgressBar::new(indices.len() as u64);
            pb.set_style(
                ProgressStyle::with_template(
                    "  {spinner:.green} [{bar:30.cyan/dim}] {pos}/{len} suites — {msg}",
                )
                .unwrap()
                .progress_chars("##-"),
            );

            for (step, &idx) in indices.iter().enumerate() {
                let suite = &self.suites[idx];
                pb.set_message(suite.name().to_string());

                let result = run_suite(suite.as_ref(), client).await;
                pb.println(format_suite_progress(&result));

                suite_results.push(result);
                pb.set_position((step + 1) as u64);
            }

            pb.finish_and_clear();
        }

        // Aggregate totals
        let (total_passed, total_failed, total_skipped) = count_totals(&suite_results);

        BattleReport {
            version: env!("CARGO_PKG_VERSION").to_string(),
            elysiandb_version: elysiandb_version.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            suites: suite_results,
            performance: Vec::new(),
            total_passed,
            total_failed,
            total_skipped,
            total_duration: total_start.elapsed(),
        }
    }

    /// Check whether a suite name matches the `--suite` filter.
    pub fn should_run(&self, suite_name: &str) -> bool {
        match &self.suite_filter {
            None => true,
            Some(filter) => {
                let normalized = suite_name.to_lowercase().replace(' ', "_");
                filter.iter().any(|f| normalized.contains(f.as_str()))
            }
        }
    }
}

/// Build the single-line progress string for a completed suite — green
/// `✓ N/M passed (Xs)` on success, red `✗ N/M passed, F failed (Xs)`
/// otherwise. Shared between `Runner::run` (internal suites) and
/// `main::run` (external crash-recovery suite) so the two output paths
/// can't drift in wording or style.
pub fn format_suite_progress(result: &SuiteResult) -> String {
    let passed = result
        .tests
        .iter()
        .filter(|t| matches!(t.status, TestStatus::Passed))
        .count();
    let failed = result
        .tests
        .iter()
        .filter(|t| matches!(t.status, TestStatus::Failed))
        .count();
    let total = result.tests.len();
    let secs = result.duration.as_secs_f64();

    if failed > 0 {
        format!(
            "  {} {} — {}/{} passed, {} failed ({:.1}s)",
            style("✗").red(),
            result.name,
            passed,
            total,
            failed,
            secs
        )
    } else {
        format!(
            "  {} {} — {}/{} passed ({:.1}s)",
            style("✓").green(),
            result.name,
            passed,
            total,
            secs
        )
    }
}

/// Execute a single suite: cleanup → setup → run → teardown.
async fn run_suite(suite: &dyn TestSuite, client: &ElysianClient) -> SuiteResult {
    let start = Instant::now();

    // Cleanup data from previous suite
    cleanup_between_suites(client).await;

    // Setup (seed data)
    if let Err(e) = suite.setup(client).await {
        warn!("Suite '{}' setup failed: {:#}", suite.name(), e);
    }

    // Run tests
    let tests = suite.run(client).await;

    // Teardown
    if let Err(e) = suite.teardown(client).await {
        warn!("Suite '{}' teardown failed: {:#}", suite.name(), e);
    }

    SuiteResult {
        name: suite.name().to_string(),
        tests,
        duration: start.elapsed(),
    }
}

/// Cleanup between suites: `DELETE /api/{entity}` for every known battle_*
/// entity to remove leftover test documents.
///
/// `POST /reset` is intentionally NOT called here even though the spec mentions
/// it. In the ElysianDB versions targeted by this harness, `/reset` wipes
/// every KV key including the admin session and the per-entity ACL grants —
/// after that, even an explicitly re-logged-in admin gets `403 Access denied`
/// on documents they own. KV-suite cleanup will be reintroduced inside the KV
/// suite itself once that suite exists.
async fn cleanup_between_suites(client: &ElysianClient) {
    for entity in BATTLE_ENTITIES {
        let _ = client.delete_all(entity).await;
    }
}

/// Append a suite result to an existing `BattleReport` and re-derive the
/// aggregate pass/fail/skipped counters. Used by the main orchestrator to
/// fold in the crash-recovery suite, which runs outside `Runner::run`
/// because it needs a mutable `ElysianInstance` reference that the
/// `TestSuite` trait does not carry.
pub fn append_suite_result(report: &mut BattleReport, extra: SuiteResult) {
    let extra_duration = extra.duration;
    report.suites.push(extra);
    let (p, f, s) = count_totals(&report.suites);
    report.total_passed = p;
    report.total_failed = f;
    report.total_skipped = s;
    report.total_duration += extra_duration;
}

fn count_totals(suites: &[SuiteResult]) -> (u64, u64, u64) {
    let (mut passed, mut failed, mut skipped) = (0u64, 0u64, 0u64);
    for suite in suites {
        for test in &suite.tests {
            match test.status {
                TestStatus::Passed => passed += 1,
                TestStatus::Failed => failed += 1,
                TestStatus::Skipped => skipped += 1,
            }
        }
    }
    (passed, failed, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suites::{TestResult, TestStatus};
    use std::time::Duration;

    #[test]
    fn count_totals_aggregates_correctly() {
        let suites = vec![
            SuiteResult {
                name: "a".into(),
                tests: vec![
                    TestResult {
                        suite: "a".into(),
                        name: "t1".into(),
                        status: TestStatus::Passed,
                        duration: Duration::ZERO,
                        error: None,
                        request: None,
                        response_status: None,
                    },
                    TestResult {
                        suite: "a".into(),
                        name: "t2".into(),
                        status: TestStatus::Failed,
                        duration: Duration::ZERO,
                        error: Some("boom".into()),
                        request: None,
                        response_status: None,
                    },
                ],
                duration: Duration::ZERO,
            },
            SuiteResult {
                name: "b".into(),
                tests: vec![TestResult {
                    suite: "b".into(),
                    name: "t3".into(),
                    status: TestStatus::Skipped,
                    duration: Duration::ZERO,
                    error: None,
                    request: None,
                    response_status: None,
                }],
                duration: Duration::ZERO,
            },
        ];
        assert_eq!(count_totals(&suites), (1, 1, 1));
    }

    #[test]
    fn should_run_no_filter_matches_all() {
        let runner = Runner::new(vec![], None);
        assert!(runner.should_run("Health & System"));
        assert!(runner.should_run("Entity CRUD"));
    }

    #[test]
    fn should_run_with_filter() {
        let runner = Runner::new(vec![], Some(vec!["crud".into(), "query".into()]));
        assert!(runner.should_run("Entity CRUD"));
        assert!(runner.should_run("Query API"));
        assert!(!runner.should_run("Health & System"));
    }
}
