use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::style;
use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::cli::ReportFormat;
use crate::suites::{BattleReport, TestStatus};

// ---- Text report -----------------------------------------------------------

#[derive(Tabled)]
struct SuiteRow {
    #[tabled(rename = "Suite")]
    name: String,
    #[tabled(rename = "Passed")]
    passed: u64,
    #[tabled(rename = "Failed")]
    failed: u64,
    #[tabled(rename = "Skipped")]
    skipped: u64,
    #[tabled(rename = "Duration")]
    duration: String,
}

#[derive(Tabled)]
struct FailedRow {
    #[tabled(rename = "Suite")]
    suite: String,
    #[tabled(rename = "Test")]
    name: String,
    #[tabled(rename = "Error")]
    error: String,
    #[tabled(rename = "Request")]
    request: String,
}

fn print_text_report(report: &BattleReport) {
    if report.suites.is_empty() {
        return;
    }

    // Suite summary table
    let rows: Vec<SuiteRow> = report
        .suites
        .iter()
        .map(|s| {
            let passed = s
                .tests
                .iter()
                .filter(|t| matches!(t.status, TestStatus::Passed))
                .count() as u64;
            let failed = s
                .tests
                .iter()
                .filter(|t| matches!(t.status, TestStatus::Failed))
                .count() as u64;
            let skipped = s
                .tests
                .iter()
                .filter(|t| matches!(t.status, TestStatus::Skipped))
                .count() as u64;
            SuiteRow {
                name: s.name.clone(),
                passed,
                failed,
                skipped,
                duration: format!("{:.1}s", s.duration.as_secs_f64()),
            }
        })
        .collect();

    println!(
        "\n  {} {}\n",
        style("──").dim(),
        style("Suite Results").bold()
    );
    let table = Table::new(&rows).with(Style::rounded()).to_string();
    for line in table.lines() {
        println!("  {line}");
    }

    // Failed tests detail
    let failed_rows: Vec<FailedRow> = report
        .suites
        .iter()
        .flat_map(|s| {
            s.tests.iter().filter_map(|t| {
                if matches!(t.status, TestStatus::Failed) {
                    Some(FailedRow {
                        suite: t.suite.clone(),
                        name: t.name.clone(),
                        error: t.error.clone().unwrap_or_default(),
                        request: t.request.clone().unwrap_or_else(|| "-".into()),
                    })
                } else {
                    None
                }
            })
        })
        .collect();

    if !failed_rows.is_empty() {
        println!(
            "\n  {} {}\n",
            style("──").dim(),
            style("Failed Tests").bold().red()
        );
        let table = Table::new(&failed_rows).with(Style::rounded()).to_string();
        for line in table.lines() {
            println!("  {line}");
        }
    }

    // Summary line
    println!("\n  {} {}\n", style("──").dim(), style("Summary").bold());

    let total = report.total_passed + report.total_failed + report.total_skipped;
    let status_text = if report.total_failed == 0 {
        style("ALL PASSED").green().bold().to_string()
    } else {
        style(format!("{} FAILED", report.total_failed))
            .red()
            .bold()
            .to_string()
    };

    println!(
        "  {} tests | {} passed | {} failed | {} skipped | {:.1}s | {}",
        total,
        report.total_passed,
        report.total_failed,
        report.total_skipped,
        report.total_duration.as_secs_f64(),
        status_text,
    );
    println!();
}

// ---- JSON report -----------------------------------------------------------

fn write_json_report(report: &BattleReport, battle_dir: &Path) -> Result<PathBuf> {
    let reports_dir = battle_dir.join("reports");
    std::fs::create_dir_all(&reports_dir).context("Failed to create .battle/reports/ directory")?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("{timestamp}.json");
    let report_path = reports_dir.join(&filename);

    let json = serde_json::to_string_pretty(report).context("Failed to serialize BattleReport")?;
    std::fs::write(&report_path, &json).context("Failed to write JSON report")?;

    // Update latest.json symlink
    let latest_path = reports_dir.join("latest.json");
    let _ = std::fs::remove_file(&latest_path);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&filename, &latest_path)
        .context("Failed to create latest.json symlink")?;

    Ok(report_path)
}

// ---- Exit code -------------------------------------------------------------

/// 0 = all passed, 1 = test failures, 2 = infrastructure error (handled upstream).
fn exit_code(report: &BattleReport) -> i32 {
    if report.total_failed > 0 {
        1
    } else {
        0
    }
}

// ---- Public entry point ----------------------------------------------------

/// Generate the full report output (text and/or JSON) and return the exit code.
pub fn generate(report: &BattleReport, format: ReportFormat, battle_dir: &Path) -> Result<i32> {
    // Always write JSON report to disk
    let json_path = write_json_report(report, battle_dir)?;

    match format {
        ReportFormat::Text => {
            print_text_report(report);
            println!(
                "  {} Report saved to {}\n",
                style("ℹ").cyan(),
                json_path.display()
            );
        }
        ReportFormat::Json => {
            println!(
                "\n  {} JSON report: {}\n",
                style("✓").green(),
                json_path.display()
            );
        }
    }

    Ok(exit_code(report))
}

// ---- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suites::{BattleReport, SuiteResult, TestResult, TestStatus};
    use std::time::Duration;

    fn sample_report(failed: u64) -> BattleReport {
        let mut tests = vec![TestResult {
            suite: "health".into(),
            name: "t1".into(),
            status: TestStatus::Passed,
            duration: Duration::from_millis(10),
            error: None,
            request: Some("GET /health".into()),
            response_status: Some(200),
        }];
        if failed > 0 {
            tests.push(TestResult {
                suite: "health".into(),
                name: "t2".into(),
                status: TestStatus::Failed,
                duration: Duration::from_millis(5),
                error: Some("expected 200, got 500".into()),
                request: Some("GET /stats".into()),
                response_status: Some(500),
            });
        }
        BattleReport {
            version: "0.1.0".into(),
            elysiandb_version: "main".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            suites: vec![SuiteResult {
                name: "Health & System".into(),
                tests,
                duration: Duration::from_millis(15),
            }],
            performance: vec![],
            total_passed: 1,
            total_failed: failed,
            total_skipped: 0,
            total_duration: Duration::from_millis(15),
        }
    }

    #[test]
    fn exit_code_zero_when_all_pass() {
        assert_eq!(exit_code(&sample_report(0)), 0);
    }

    #[test]
    fn exit_code_one_when_failures() {
        assert_eq!(exit_code(&sample_report(1)), 1);
    }

    #[test]
    fn json_report_writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let report = sample_report(0);
        let path = write_json_report(&report, dir.path()).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], "0.1.0");
    }

    #[test]
    fn json_report_creates_latest_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let report = sample_report(0);
        write_json_report(&report, dir.path()).unwrap();
        let latest = dir.path().join("reports/latest.json");
        assert!(latest.exists());
    }
}
