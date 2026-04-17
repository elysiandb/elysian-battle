mod builder;
mod cli;
#[allow(dead_code)]
mod client;
mod config;
mod git;
mod instance;
mod port;
mod prerequisites;
mod report;
mod runner;
mod suites;
#[allow(dead_code)]
mod tcp_client;

use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::Parser;
use console::style;
use tracing::info;

use cli::Cli;

fn init_tracing(verbose: bool) {
    let filter = if verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

fn print_banner() {
    let banner = format!(
        "{} v{}",
        style("ELYSIAN-BATTLE").bold().cyan(),
        env!("CARGO_PKG_VERSION")
    );
    println!("\n  {banner}\n");
}

async fn run(cli: Cli) -> Result<()> {
    print_banner();
    init_tracing(cli.verbose);

    // Step 1 — Check prerequisites
    info!("Checking prerequisites...");
    let prereqs = prerequisites::check_prerequisites()?;
    println!("  {} Git {}", style("✓").green(), prereqs.git_version);
    println!("  {} Go  {}", style("✓").green(), prereqs.go_version);
    println!();

    // Step 2 — Clone / fetch repository (needed before version selection)
    let battle_dir = PathBuf::from(".battle");
    std::fs::create_dir_all(&battle_dir).context("Failed to create .battle/ directory")?;
    info!("Cloning / fetching ElysianDB repository...");
    let repo_path = git::clone_or_fetch(&battle_dir)?;
    println!();

    // Step 3 — Resolve version (interactive with real branches/tags if needed)
    let refs = git::list_refs(&repo_path)?;
    let version_ref = cli.resolve_version_interactive(&refs.branches, &refs.tags)?;
    info!("Target version: {version_ref}");

    // Step 4 — Find available ports
    let ports = port::find_available_ports()?;
    info!(
        "Ports selected — HTTP: {}, TCP: {}",
        ports.http_port, ports.tcp_port
    );

    // Step 5 — Checkout target version
    let repo_info = git::checkout(&repo_path, &version_ref)?;
    info!("Checked out: {}", repo_info.checked_out_ref);
    println!();

    // Step 6 — Build ElysianDB binary
    let build_result = builder::build_elysiandb(&repo_path, &battle_dir, cli.no_build)?;
    if !build_result.skipped {
        info!(
            "Binary ready: {} ({:.1}s)",
            build_result.binary_path.display(),
            build_result.duration_secs
        );
    }
    println!();

    // Step 7 — Generate elysian.yaml config
    config::generate_config(&battle_dir, &ports)?;

    // Step 8+9 — Start ElysianDB process and health check
    let mut instance = instance::ElysianInstance::start(&battle_dir, ports.http_port).await?;

    // Step 10 — Smoke-test both clients against the live instance
    smoke_test(ports.http_port, ports.tcp_port).await?;

    // Step 11 — Run test suites
    let http_client = client::ElysianClient::new(ports.http_port);
    let login_resp = http_client.login("admin", "admin").await?;
    anyhow::ensure!(
        login_resp.status().is_success(),
        "Runner login failed: {}",
        login_resp.status()
    );

    let all_suites = suites::all_suites(ports.tcp_port);
    let runner = runner::Runner::new(all_suites, cli.parse_suites());
    let mut battle_report = runner.run(&http_client, &repo_info.checked_out_ref).await;

    // Step 11b — Crash recovery suite runs outside the Runner because it
    // needs a mutable `ElysianInstance` reference (kill_hard + restart).
    // The runner's --suite filter is honored via `should_run`, so
    // `--suite crash_recovery` still works end-to-end.
    if runner.should_run("Crash Recovery") {
        println!();
        let result = suites::crash_recovery::run_crash_recovery(&mut instance, &http_client).await;
        let (passed, failed) = summarize_suite(&result);
        if failed > 0 {
            println!(
                "  {} {} — {}/{} passed, {} failed ({:.1}s)",
                style("✗").red(),
                result.name,
                passed,
                result.tests.len(),
                failed,
                result.duration.as_secs_f64()
            );
        } else {
            println!(
                "  {} {} — {}/{} passed ({:.1}s)",
                style("✓").green(),
                result.name,
                passed,
                result.tests.len(),
                result.duration.as_secs_f64()
            );
        }
        runner::append_suite_result(&mut battle_report, result);
    }

    // Step 12 — Stop ElysianDB
    if !cli.keep_alive {
        instance.stop().await?;
    } else {
        println!(
            "  {} ElysianDB left running (--keep-alive) on port {}",
            style("ℹ").cyan(),
            ports.http_port,
        );
    }

    // Step 13 — Generate report and derive exit code
    let exit_code = report::generate(&battle_report, cli.report, &battle_dir)?;
    if exit_code != 0 {
        process::exit(exit_code);
    }

    Ok(())
}

/// Count passed / failed tests in a single `SuiteResult`. Mirrors the
/// progress-line formatting the runner uses for registered suites so the
/// crash-recovery output reads the same way.
fn summarize_suite(result: &suites::SuiteResult) -> (usize, usize) {
    use suites::TestStatus;
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
    (passed, failed)
}

/// Quick smoke test exercising both clients against the live ElysianDB instance.
async fn smoke_test(http_port: u16, tcp_port: u16) -> Result<()> {
    use client::ElysianClient;
    use tcp_client::ElysianTcpClient;

    println!("  {} Running client smoke test...", style("~").yellow());

    // ---- HTTP client ----
    let http = ElysianClient::new(http_port);

    // Login
    let resp = http.login("admin", "admin").await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "login failed: {}",
        resp.status()
    );
    println!("    {} HTTP login", style("✓").green());

    // Health
    let resp = http.health().await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "health failed: {}",
        resp.status()
    );
    println!("    {} HTTP health", style("✓").green());

    // Create entity
    let doc = serde_json::json!({"title": "Smoke Test", "value": 42});
    let resp = http.create("battle_smoke", doc).await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "create failed: {}",
        resp.status()
    );
    let created: serde_json::Value = resp.json().await?;
    let id = created["id"]
        .as_str()
        .context("missing id in create response")?;
    println!("    {} HTTP create (id={})", style("✓").green(), id);

    // Read back
    let resp = http.get("battle_smoke", id).await?;
    anyhow::ensure!(resp.status().is_success(), "get failed: {}", resp.status());
    println!("    {} HTTP get", style("✓").green());

    // Delete
    let resp = http.delete("battle_smoke", id).await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "delete failed: {}",
        resp.status()
    );
    println!("    {} HTTP delete", style("✓").green());

    // Cleanup entity
    let _ = http.delete_all("battle_smoke").await;

    // KV via HTTP
    let resp = http.kv_set("battle_smoke_key", "hello", None).await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "kv_set failed: {}",
        resp.status()
    );
    let resp = http.kv_get("battle_smoke_key").await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "kv_get failed: {}",
        resp.status()
    );
    let resp = http.kv_delete("battle_smoke_key").await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "kv_delete failed: {}",
        resp.status()
    );
    println!("    {} HTTP kv set/get/delete", style("✓").green());

    // ---- TCP client ----
    let mut tcp = ElysianTcpClient::connect(tcp_port).await?;

    let pong = tcp.ping().await?;
    anyhow::ensure!(pong == "PONG", "expected PONG, got: {pong}");
    println!("    {} TCP PING → PONG", style("✓").green());

    let ok = tcp.set("battle_smoke_tcp", "world").await?;
    anyhow::ensure!(ok == "OK", "expected OK, got: {ok}");

    let val = tcp.get("battle_smoke_tcp").await?;
    anyhow::ensure!(
        val == "battle_smoke_tcp=world",
        "expected 'battle_smoke_tcp=world', got: {val}"
    );
    println!("    {} TCP SET/GET", style("✓").green());

    let _ = tcp.del("battle_smoke_tcp").await?;
    println!("    {} TCP DEL", style("✓").green());

    println!("  {} Client smoke test passed", style("✓").green().bold());
    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(err) = run(cli).await {
        eprintln!("{} {:#}", style("Error:").red().bold(), err);
        process::exit(2);
    }
}
