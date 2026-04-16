use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use console::style;
use tracing::info;

pub struct BuildResult {
    pub binary_path: PathBuf,
    pub duration_secs: f64,
    pub skipped: bool,
}

/// Build ElysianDB from source, or skip if `--no-build` and binary exists.
pub fn build_elysiandb(repo_path: &Path, battle_dir: &Path, no_build: bool) -> Result<BuildResult> {
    let bin_dir = battle_dir.join("bin");
    let binary_path = bin_dir.join("elysiandb");

    if no_build && binary_path.exists() {
        println!(
            "  {} Build skipped (--no-build, binary exists)",
            style("⊘").yellow()
        );
        return Ok(BuildResult {
            binary_path,
            duration_secs: 0.0,
            skipped: true,
        });
    }

    if no_build && !binary_path.exists() {
        info!("--no-build specified but no binary found, building anyway");
    }

    std::fs::create_dir_all(&bin_dir).context("Failed to create .battle/bin/ directory")?;

    // Compute absolute output path for -o flag
    let abs_binary_path = std::fs::canonicalize(&bin_dir)
        .context("Failed to resolve .battle/bin/ path")?
        .join("elysiandb");

    println!("  {} Building ElysianDB...", style("⟳").yellow());
    let start = Instant::now();

    let output = Command::new("go")
        .args([
            "build",
            "-trimpath",
            "-ldflags=-s -w",
            "-o",
            abs_binary_path.to_str().unwrap_or("elysiandb"),
            ".",
        ])
        .env("CGO_ENABLED", "0")
        .current_dir(repo_path)
        .output()
        .context("Failed to run go build — is Go installed?")?;

    let duration = start.elapsed();
    let duration_secs = duration.as_secs_f64();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Go build failed (exit code {}):\n\n{stderr}",
            output.status.code().unwrap_or(-1)
        );
    }

    println!("  {} Built in {:.1}s", style("✓").green(), duration_secs);

    Ok(BuildResult {
        binary_path,
        duration_secs,
        skipped: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_no_build_skips_when_binary_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let battle_dir = tmp.path();
        let bin_dir = battle_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("elysiandb"), b"fake-binary").unwrap();

        let result = build_elysiandb(tmp.path(), battle_dir, true).unwrap();
        assert!(result.skipped);
        assert_eq!(result.duration_secs, 0.0);
    }
}
