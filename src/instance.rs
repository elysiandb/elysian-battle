use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use console::style;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Instant};
use tracing::info;

/// Handle to a running ElysianDB instance.
pub struct ElysianInstance {
    child: Child,
    pub http_port: u16,
    log_path: PathBuf,
}

impl ElysianInstance {
    /// Start ElysianDB, wait for it to become healthy, and return the instance handle.
    pub async fn start(battle_dir: &Path, http_port: u16) -> Result<Self> {
        let binary_path = battle_dir.join("bin/elysiandb");
        let config_path = battle_dir.join("config/elysian.yaml");
        let log_path = battle_dir.join("logs/elysiandb.log");

        std::fs::create_dir_all(battle_dir.join("logs"))
            .context("Failed to create .battle/logs/ directory")?;

        // Wipe data directory for a clean run
        let data_dir = battle_dir.join("data");
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir).context("Failed to wipe .battle/data/")?;
        }

        let log_file =
            std::fs::File::create(&log_path).context("Failed to create elysiandb.log")?;
        let log_stderr = log_file
            .try_clone()
            .context("Failed to clone log file handle")?;

        println!("  {} Starting ElysianDB...", style("⟳").yellow());

        let child = Command::new(&binary_path)
            .arg("server")
            .arg("-config")
            .arg(&config_path)
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_stderr))
            .kill_on_drop(false)
            .spawn()
            .context("Failed to spawn ElysianDB process")?;

        let pid = child.id().unwrap_or(0);
        info!("ElysianDB process started (PID: {pid})");

        let mut instance = Self {
            child,
            http_port,
            log_path,
        };

        if let Err(e) = instance.wait_for_health().await {
            // Try to clean up the process before returning the error
            let _ = instance.kill_hard().await;
            return Err(e);
        }

        Ok(instance)
    }

    /// Poll `GET /health` until 200 or timeout (30s).
    ///
    /// In `user` auth mode, `/health` requires an authenticated session.
    /// We first wait for the server to accept TCP connections, then POST
    /// `/api/security/login` with admin/admin to obtain a session cookie,
    /// and finally hit `/health` with that cookie.
    async fn wait_for_health(&self) -> Result<()> {
        let base = format!("http://127.0.0.1:{}", self.http_port);
        let health_url = format!("{base}/health");
        let login_url = format!("{base}/api/security/login");
        let timeout = Duration::from_secs(30);
        let interval = Duration::from_millis(500);
        let deadline = Instant::now() + timeout;

        // Cookie jar keeps the session cookie from login
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(2))
            .build()
            .context("Failed to create HTTP client")?;

        // Phase 1: wait for the server to respond at all (any status on /health)
        loop {
            if Instant::now() > deadline {
                self.print_log_tail(50);
                bail!(
                    "Health check timed out after 30s — ElysianDB did not respond on port {}. \
                     Check log output above for details.",
                    self.http_port
                );
            }

            if client.get(&health_url).send().await.is_ok() {
                break;
            }
            sleep(interval).await;
        }

        // Phase 2: authenticate to get a session cookie
        let login_body = serde_json::json!({
            "username": "admin",
            "password": "admin"
        });
        let login_resp = client
            .post(&login_url)
            .json(&login_body)
            .send()
            .await
            .context("Failed to send login request")?;

        if !login_resp.status().is_success() {
            self.print_log_tail(50);
            bail!(
                "Login to ElysianDB failed (status {}). Check log output above.",
                login_resp.status()
            );
        }
        info!("Authenticated with ElysianDB (admin)");

        // Phase 3: health check with session cookie
        let resp = client
            .get(&health_url)
            .send()
            .await
            .context("Health check request failed after login")?;

        if resp.status().as_u16() != 200 {
            self.print_log_tail(50);
            bail!(
                "Health check returned {} after login — expected 200",
                resp.status()
            );
        }

        println!("  {} Health check passed", style("✓").green());
        Ok(())
    }

    /// Print the last `n` lines of the ElysianDB log file to stderr.
    fn print_log_tail(&self, n: usize) {
        eprintln!(
            "\n  {} Last {n} lines of {}:",
            style("⚠").yellow(),
            self.log_path.display()
        );

        match std::fs::File::open(&self.log_path) {
            Ok(file) => {
                let lines: Vec<String> = std::io::BufReader::new(file)
                    .lines()
                    .map_while(Result::ok)
                    .collect();
                let start = lines.len().saturating_sub(n);
                for line in &lines[start..] {
                    eprintln!("    {line}");
                }
            }
            Err(e) => {
                eprintln!("    (could not read log file: {e})");
            }
        }
        eprintln!();
    }

    /// Graceful shutdown: SIGTERM, then SIGKILL after 5s.
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping ElysianDB (graceful)...");

        let pid = match self.child.id() {
            Some(pid) => pid,
            None => {
                info!("Process already exited");
                return Ok(());
            }
        };

        // Send SIGTERM
        let raw_pid = nix::unistd::Pid::from_raw(pid as i32);
        if let Err(e) = nix::sys::signal::kill(raw_pid, nix::sys::signal::Signal::SIGTERM) {
            info!("SIGTERM failed ({e}), process may have already exited");
            let _ = self.child.wait().await;
            return Ok(());
        }

        // Wait up to 5 seconds for clean exit
        match tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(status)) => {
                info!("ElysianDB exited cleanly: {status}");
                println!("  {} ElysianDB stopped", style("✓").green());
            }
            _ => {
                info!("Graceful shutdown timed out, sending SIGKILL");
                self.child
                    .kill()
                    .await
                    .context("Failed to SIGKILL ElysianDB")?;
                let _ = self.child.wait().await;
                println!(
                    "  {} ElysianDB force-killed (SIGTERM timed out)",
                    style("⚠").yellow()
                );
            }
        }

        Ok(())
    }

    /// Immediate SIGKILL without graceful shutdown — for crash recovery tests.
    pub async fn kill_hard(&mut self) -> Result<()> {
        info!("Sending SIGKILL to ElysianDB (hard kill)...");
        self.child
            .kill()
            .await
            .context("Failed to SIGKILL ElysianDB")?;
        let _ = self.child.wait().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_tail_missing_file() {
        let instance = ElysianInstance {
            child: {
                // Create a dummy child by spawning a short-lived process
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async { Command::new("true").spawn().unwrap() })
            },
            http_port: 0,
            log_path: PathBuf::from("/nonexistent/path/log.txt"),
        };
        // Should not panic — just prints an error message
        instance.print_log_tail(10);
    }
}
