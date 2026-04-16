mod cli;
mod port;
mod prerequisites;

use std::process;

use anyhow::Result;
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

    // Step 2 — Resolve version (interactive if needed)
    let version_ref = cli.resolve_version_interactive()?;
    info!("Target version: {version_ref}");

    // Step 3 — Find available ports
    let ports = port::find_available_ports()?;
    info!(
        "Ports selected — HTTP: {}, TCP: {}",
        ports.http_port, ports.tcp_port
    );

    // Step 4 — Clone / fetch repository
    // TODO: implement in src/git.rs (ticket #2)

    // Step 5 — Checkout target version
    // TODO: implement in src/git.rs (ticket #2)

    // Step 6 — Build ElysianDB binary
    // TODO: implement in src/builder.rs (ticket #3)

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
        "  {} Skeleton ready — pipeline steps 4-12 are stubs for future tickets.",
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
