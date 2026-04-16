use std::time::Instant;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::{info, warn};

use crate::client::ElysianClient;
use crate::suites::{BattleReport, SuiteResult, TestStatus, TestSuite, BATTLE_ENTITIES};

/// Test orchestrator — discovers, filters, and executes suites sequentially.
pub struct Runner {
    suites: Vec<Box<dyn TestSuite>>,
    suite_filter: Option<Vec<String>>,
}

impl Runner {
    pub fn new(suites: Vec<Box<dyn TestSuite>>, suite_filter: Option<Vec<String>>) -> Self {
        Self {
            suites,
            suite_filter,
        }
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
            println!("\n  {} No test suites to run", style("ℹ").cyan());
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

                if failed > 0 {
                    pb.println(format!(
                        "  {} {} — {}/{} passed, {} failed ({:.1}s)",
                        style("✗").red(),
                        suite.name(),
                        passed,
                        total,
                        failed,
                        result.duration.as_secs_f64()
                    ));
                } else {
                    pb.println(format!(
                        "  {} {} — {}/{} passed ({:.1}s)",
                        style("✓").green(),
                        suite.name(),
                        passed,
                        total,
                        result.duration.as_secs_f64()
                    ));
                }

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
    fn should_run(&self, suite_name: &str) -> bool {
        match &self.suite_filter {
            None => true,
            Some(filter) => {
                let normalized = suite_name.to_lowercase().replace(' ', "_");
                filter.iter().any(|f| normalized.contains(f.as_str()))
            }
        }
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

/// Two-part cleanup between suites:
/// 1. `POST /reset` — clears all KV keys
/// 2. `DELETE /api/{entity}` — removes documents for each known battle_* entity
async fn cleanup_between_suites(client: &ElysianClient) {
    if let Err(e) = client.reset().await {
        info!("KV reset between suites failed (non-fatal): {:#}", e);
    }

    for entity in BATTLE_ENTITIES {
        let _ = client.delete_all(entity).await;
    }
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
