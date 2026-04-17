use std::path::Path;

use anyhow::{Context, Result};
use console::style;
use serde::Serialize;
use tracing::info;

use crate::port::AvailablePorts;

/// Auth token used for all test requests.
pub const BATTLE_TOKEN: &str = "battle-test-token-2026";

#[derive(Serialize)]
pub struct ElysianConfig {
    pub store: Store,
    pub engine: Engine,
    pub server: Server,
    pub log: Log,
    pub stats: Stats,
    pub security: Security,
    pub api: Api,
    pub adminui: AdminUi,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub folder: String,
    pub shards: u32,
    pub flush_interval_seconds: u32,
    pub crash_recovery: CrashRecovery,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashRecovery {
    pub enabled: bool,
    #[serde(rename = "maxLogMB")]
    pub max_log_mb: u32,
}

#[derive(Serialize)]
pub struct Engine {
    pub name: String,
}

#[derive(Serialize)]
pub struct Server {
    pub http: ServerEndpoint,
    pub tcp: ServerEndpoint,
}

#[derive(Serialize)]
pub struct ServerEndpoint {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    pub flush_interval_seconds: u32,
}

#[derive(Serialize)]
pub struct Stats {
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct Security {
    pub authentication: Authentication,
}

#[derive(Serialize)]
pub struct Authentication {
    pub enabled: bool,
    pub mode: String,
    pub token: String,
}

#[derive(Serialize)]
pub struct Api {
    pub schema: Schema,
    pub index: Index,
    pub cache: Cache,
    pub hooks: Hooks,
}

#[derive(Serialize)]
pub struct Schema {
    pub enabled: bool,
    pub strict: bool,
}

#[derive(Serialize)]
pub struct Index {
    pub workers: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cache {
    pub enabled: bool,
    pub cleanup_interval_seconds: u32,
}

#[derive(Serialize)]
pub struct Hooks {
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct AdminUi {
    pub enabled: bool,
}

impl ElysianConfig {
    /// Build the canonical test configuration for given ports.
    pub fn new(ports: &AvailablePorts) -> Self {
        Self {
            store: Store {
                folder: ".battle/data".to_string(),
                shards: 64,
                flush_interval_seconds: 30,
                crash_recovery: CrashRecovery {
                    enabled: true,
                    max_log_mb: 50,
                },
            },
            engine: Engine {
                name: "internal".to_string(),
            },
            server: Server {
                http: ServerEndpoint {
                    enabled: true,
                    host: "127.0.0.1".to_string(),
                    port: ports.http_port,
                },
                tcp: ServerEndpoint {
                    enabled: true,
                    host: "127.0.0.1".to_string(),
                    port: ports.tcp_port,
                },
            },
            log: Log {
                flush_interval_seconds: 5,
            },
            stats: Stats { enabled: true },
            security: Security {
                authentication: Authentication {
                    enabled: true,
                    mode: "user".to_string(),
                    token: BATTLE_TOKEN.to_string(),
                },
            },
            api: Api {
                schema: Schema {
                    enabled: true,
                    // ElysianDB v0.1.14 gates per-entity strict-mode validation
                    // (`internal/api/storage.go:WriteEntity` →
                    // `internal/schema/analyzer.go:ValidateEntity`) on BOTH the
                    // global flag AND `_manual: true` on the entity schema.
                    // Suite 6 (Schema) tests S-05 (strict rejects undeclared
                    // field) and S-06 (required field missing) need strict
                    // enforcement to actually fire, so the harness enables the
                    // global flag here. With no manual schema, validation
                    // collapses to type-only checks (which is what every other
                    // suite already relies on for auto-inferred entities).
                    strict: true,
                },
                index: Index { workers: 2 },
                cache: Cache {
                    enabled: true,
                    cleanup_interval_seconds: 5,
                },
                hooks: Hooks { enabled: true },
            },
            adminui: AdminUi { enabled: false },
        }
    }
}

/// Generate the elysian.yaml config file in `.battle/config/`.
pub fn generate_config(battle_dir: &Path, ports: &AvailablePorts) -> Result<()> {
    let config_dir = battle_dir.join("config");
    std::fs::create_dir_all(&config_dir).context("Failed to create .battle/config/ directory")?;

    let config_path = config_dir.join("elysian.yaml");
    let config = ElysianConfig::new(ports);
    let yaml = serde_yaml::to_string(&config).context("Failed to serialize config to YAML")?;

    std::fs::write(&config_path, &yaml).context("Failed to write elysian.yaml")?;

    info!("Config written to {}", config_path.display());
    println!(
        "  {} Config generated (HTTP:{}, TCP:{})",
        style("✓").green(),
        ports.http_port,
        ports.tcp_port,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization_has_expected_fields() {
        let ports = AvailablePorts {
            http_port: 9000,
            tcp_port: 9001,
        };
        let config = ElysianConfig::new(&ports);
        let yaml = serde_yaml::to_string(&config).unwrap();

        assert!(yaml.contains("folder: .battle/data"));
        assert!(yaml.contains("shards: 64"));
        assert!(yaml.contains("port: 9000"));
        assert!(yaml.contains("port: 9001"));
        assert!(yaml.contains("mode: user"));
        assert!(yaml.contains("token: battle-test-token-2026"));
        assert!(yaml.contains("name: internal"));
        assert!(yaml.contains("strict: true"));
        assert!(yaml.contains("flushIntervalSeconds: 30"));
        assert!(yaml.contains("maxLogMB: 50"));
    }

    #[test]
    fn test_generate_config_writes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ports = AvailablePorts {
            http_port: 8080,
            tcp_port: 8081,
        };

        generate_config(tmp.path(), &ports).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("config/elysian.yaml")).unwrap();
        assert!(content.contains("port: 8080"));
        assert!(content.contains("port: 8081"));
    }
}
