use std::process::Command;

use anyhow::{bail, Context, Result};

pub struct Prerequisites {
    pub git_version: String,
    pub go_version: String,
}

pub fn check_prerequisites() -> Result<Prerequisites> {
    let git_version = check_git()?;
    let go_version = check_go()?;
    Ok(Prerequisites {
        git_version,
        go_version,
    })
}

fn check_git() -> Result<String> {
    let output = Command::new("git").arg("--version").output().context(
        "Git is not installed.\n\
             Please install Git: https://git-scm.com/downloads",
    )?;

    if !output.status.success() {
        bail!(
            "git --version failed with exit code {}.\n\
             Please verify your Git installation.",
            output.status
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // "git version 2.43.0" → "2.43.0"
    let version = stdout
        .trim()
        .strip_prefix("git version ")
        .unwrap_or(stdout.trim())
        .to_string();

    Ok(version)
}

fn check_go() -> Result<String> {
    let output = Command::new("go").arg("version").output().context(
        "Go toolchain not found.\n\
             Please install Go 1.24+: https://golang.org/dl\n\
             macOS: brew install go\n\
             Linux: sudo apt install golang / sudo dnf install golang",
    )?;

    if !output.status.success() {
        bail!(
            "go version failed with exit code {}.\n\
             Please verify your Go installation.",
            output.status
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // "go version go1.24.2 darwin/arm64" → "1.24.2"
    let version = parse_go_version(&stdout)?;
    validate_go_version(&version)?;

    Ok(version)
}

/// Extracts the semver string from `go version` output.
/// Input: "go version go1.24.2 darwin/arm64"
/// Output: "1.24.2"
fn parse_go_version(output: &str) -> Result<String> {
    let go_prefix = output
        .split_whitespace()
        .find(|s| s.starts_with("go1") || s.starts_with("go2"))
        .context(format!(
            "Could not parse Go version from output: {output}\n\
             Expected format: go version go1.X.Y <os>/<arch>"
        ))?;

    Ok(go_prefix
        .strip_prefix("go")
        .unwrap_or(go_prefix)
        .to_string())
}

/// Validates that Go version is >= 1.24.
fn validate_go_version(version: &str) -> Result<()> {
    let parts: Vec<&str> = version.split('.').collect();
    let major: u32 = parts
        .first()
        .unwrap_or(&"0")
        .parse()
        .context("Invalid Go major version")?;
    let minor: u32 = parts
        .get(1)
        .unwrap_or(&"0")
        .parse()
        .context("Invalid Go minor version")?;

    if major < 1 || (major == 1 && minor < 24) {
        bail!(
            "Go {version} is too old. ElysianDB requires Go 1.24+.\n\
             Please upgrade: https://golang.org/dl"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_go_version_standard() {
        let v = parse_go_version("go version go1.24.2 darwin/arm64").unwrap();
        assert_eq!(v, "1.24.2");
    }

    #[test]
    fn test_parse_go_version_linux() {
        let v = parse_go_version("go version go1.24.0 linux/amd64").unwrap();
        assert_eq!(v, "1.24.0");
    }

    #[test]
    fn test_validate_go_version_ok() {
        assert!(validate_go_version("1.24.0").is_ok());
        assert!(validate_go_version("1.25.1").is_ok());
        assert!(validate_go_version("2.0.0").is_ok());
    }

    #[test]
    fn test_validate_go_version_too_old() {
        assert!(validate_go_version("1.23.0").is_err());
        assert!(validate_go_version("1.22.5").is_err());
    }
}
