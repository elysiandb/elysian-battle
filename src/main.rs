mod builder;
mod cli;
mod git;
mod port;
mod prerequisites;

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
    // TODO: implement in src/config.rs (ticket #4)

    // Step 8 — Start ElysianDB process
    // TODO: implement in src/instance.rs (ticket #5)

    // Step 9 — Health check
    // TODO: implement in src/instance.rs (ticket #5)

    // Step 10 — Run test suites
    // TODO: implement in src/runner.rs (ticket #6+)

    // Step 11 — Stop ElysianDB
    // TODO: implement in src/instance.rs (ticket #5)

    // Step 12 — Generate report
    // TODO: implement in src/report.rs (ticket #7)

    if let Some(suites) = cli.parse_suites() {
        info!("Suite filter: {:?}", suites);
    }

    println!(
        "  {} Pipeline steps 7-12 are stubs for future tickets.",
        style("✓").green()
    );

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
