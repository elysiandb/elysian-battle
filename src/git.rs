use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;
use tracing::info;

const REPO_URL: &str = "https://github.com/elysiandb/elysiandb.git";

pub struct RepoInfo {
    pub checked_out_ref: String,
}

pub struct AvailableRefs {
    pub branches: Vec<String>,
    pub tags: Vec<String>,
}

/// Clone the ElysianDB repo if not present, then fetch latest refs.
/// Returns the path to the cloned repository.
pub fn clone_or_fetch(battle_dir: &Path) -> Result<PathBuf> {
    let repo_path = battle_dir.join("elysiandb");

    if repo_path.join(".git").exists() {
        info!("Repository exists, fetching latest refs...");
        let output = Command::new("git")
            .args(["fetch", "--all", "--tags", "--prune"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git fetch")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git fetch failed:\n{stderr}");
        }
        println!("  {} Repository updated", style("✓").green());
    } else {
        println!("  {} Cloning ElysianDB repository...", style("⟳").yellow());
        let output = Command::new("git")
            .args(["clone", REPO_URL, repo_path.to_str().unwrap_or(".")])
            .output()
            .context("Failed to clone ElysianDB repository")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "git clone failed:\n{stderr}\n\
                 Please check your network connection and try again."
            );
        }
        println!("  {} Repository cloned", style("✓").green());
    }

    Ok(repo_path)
}

/// List remote branches and tags from the cloned repository.
pub fn list_refs(repo_path: &Path) -> Result<AvailableRefs> {
    let branches = list_branches(repo_path)?;
    let tags = list_tags(repo_path)?;
    Ok(AvailableRefs { branches, tags })
}

fn list_branches(repo_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["branch", "-r", "--format=%(refname:short)"])
        .current_dir(repo_path)
        .output()
        .context("Failed to list remote branches")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch -r failed:\n{stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches: Vec<String> = stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|b| !b.is_empty())
        .filter(|b| !b.contains("HEAD"))
        .map(|b| b.strip_prefix("origin/").unwrap_or(&b).to_string())
        .collect();

    Ok(branches)
}

fn list_tags(repo_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["tag", "--list", "--sort=-v:refname"])
        .current_dir(repo_path)
        .output()
        .context("Failed to list tags")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git tag --list failed:\n{stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tags: Vec<String> = stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    Ok(tags)
}

/// Checkout a specific ref (branch, tag, or commit SHA).
pub fn checkout(repo_path: &Path, ref_name: &str) -> Result<RepoInfo> {
    let target = resolve_ref(ref_name);

    // Detach HEAD first to avoid branch conflicts
    let output = Command::new("git")
        .args(["checkout", "--detach"])
        .current_dir(repo_path)
        .output()
        .context("Failed to detach HEAD")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git checkout --detach failed:\n{stderr}");
    }

    // Now checkout the target ref
    let checkout_args = if is_branch(repo_path, &target) {
        vec!["checkout".to_string(), format!("origin/{target}")]
    } else {
        vec!["checkout".to_string(), target.clone()]
    };

    let str_args: Vec<&str> = checkout_args.iter().map(|s| s.as_str()).collect();
    let output = Command::new("git")
        .args(&str_args)
        .current_dir(repo_path)
        .output()
        .context(format!("Failed to checkout ref '{target}'"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Could not checkout '{target}':\n{stderr}\n\
             Make sure the branch, tag, or commit SHA exists."
        );
    }

    let checked_out_ref = detect_current_ref(repo_path)?;
    println!(
        "  {} Checked out: {}",
        style("✓").green(),
        style(&checked_out_ref).cyan()
    );

    Ok(RepoInfo { checked_out_ref })
}

fn resolve_ref(ref_name: &str) -> String {
    match ref_name {
        "latest" => "main".to_string(),
        other => other.to_string(),
    }
}

fn is_branch(repo_path: &Path, name: &str) -> bool {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("origin/{name}")])
        .current_dir(repo_path)
        .output();

    matches!(output, Ok(o) if o.status.success())
}

/// Detect the current checked-out ref for display/reporting.
pub fn detect_current_ref(repo_path: &Path) -> Result<String> {
    // Try symbolic ref first (branch name)
    let output = Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to detect current ref")?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Ok(branch);
        }
    }

    // Fallback: describe with tags
    let output = Command::new("git")
        .args(["describe", "--tags", "--exact-match", "HEAD"])
        .current_dir(repo_path)
        .output();

    if let Ok(ref o) = output {
        if o.status.success() {
            let tag = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !tag.is_empty() {
                return Ok(format!("tag:{tag}"));
            }
        }
    }

    // Fallback: short commit SHA
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to get HEAD commit SHA")?;

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(format!("commit:{sha}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_ref_latest() {
        assert_eq!(resolve_ref("latest"), "main");
    }

    #[test]
    fn test_resolve_ref_branch() {
        assert_eq!(resolve_ref("develop"), "develop");
    }

    #[test]
    fn test_resolve_ref_tag() {
        assert_eq!(resolve_ref("v1.0.0"), "v1.0.0");
    }
}
