use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;

use crate::client::ElysianClient;

mod crud;
mod health;
mod query;
mod query_params;

// ---- Status & result types ------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub suite: String,
    pub name: String,
    pub status: TestStatus,
    #[serde(serialize_with = "ser_duration_ms")]
    pub duration: Duration,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_status: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuiteResult {
    pub name: String,
    pub tests: Vec<TestResult>,
    #[serde(serialize_with = "ser_duration_ms")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceResult {
    pub scenario: String,
    pub iterations: u64,
    #[serde(serialize_with = "ser_duration_ms")]
    pub p50: Duration,
    #[serde(serialize_with = "ser_duration_ms")]
    pub p95: Duration,
    #[serde(serialize_with = "ser_duration_ms")]
    pub p99: Duration,
    pub throughput: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BattleReport {
    pub version: String,
    pub elysiandb_version: String,
    pub timestamp: String,
    pub suites: Vec<SuiteResult>,
    pub performance: Vec<PerformanceResult>,
    pub total_passed: u64,
    pub total_failed: u64,
    pub total_skipped: u64,
    #[serde(serialize_with = "ser_duration_ms")]
    pub total_duration: Duration,
}

fn ser_duration_ms<S>(d: &Duration, s: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_u64(d.as_millis() as u64)
}

// ---- TestSuite trait -------------------------------------------------------

#[async_trait]
#[allow(dead_code)]
pub trait TestSuite: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    async fn setup(&self, client: &ElysianClient) -> Result<()>;
    async fn run(&self, client: &ElysianClient) -> Vec<TestResult>;
    async fn teardown(&self, client: &ElysianClient) -> Result<()>;
}

// ---- Known battle_* entities (for cleanup between suites) ------------------

pub const BATTLE_ENTITIES: &[&str] = &[
    "battle_books",
    "battle_empty",
    "battle_authors",
    "battle_articles",
    "battle_tags",
    "battle_schema_test",
    "battle_auth_data",
    "battle_acl_data",
    "battle_tx_items",
    "battle_export_test",
    "battle_hooked_entity",
    "battle_posts",
    "battle_comments",
    "battle_users_nested",
    "battle_migrate_test",
    "battle_edge_unicode",
    "battle_edge_long",
    "battle_edge_deep",
    "battle_edge_concurrent",
    "battle_edge_precision",
    "battle_crash_data",
    "battle_perf_items",
    "battle_smoke",
];

// ---- Suite registration ----------------------------------------------------

/// Build the ordered list of all test suites.
///
/// Suite execution order:
///   1. Functional suites (health, crud, query, ... edge_cases)
///   2. Crash recovery (kills + restarts ElysianDB)
///   3. Performance (metrics-only, not pass/fail)
///
/// Individual suite implementations are added in subsequent tickets.
pub fn all_suites(_tcp_port: u16) -> Vec<Box<dyn TestSuite>> {
    vec![
        Box::new(health::HealthSuite),
        Box::new(crud::CrudSuite),
        Box::new(query::QuerySuite),
        Box::new(query_params::QueryParamsSuite),
    ]
}

// ---- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_serializes_to_json() {
        let result = TestResult {
            suite: "health".to_string(),
            name: "health_check".to_string(),
            status: TestStatus::Passed,
            duration: Duration::from_millis(42),
            error: None,
            request: Some("GET /health".to_string()),
            response_status: Some(200),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"duration\":42"));
        assert!(json.contains("\"status\":\"passed\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn battle_report_serializes_to_json() {
        let report = BattleReport {
            version: "0.1.0".to_string(),
            elysiandb_version: "main".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            suites: vec![],
            performance: vec![],
            total_passed: 0,
            total_failed: 0,
            total_skipped: 0,
            total_duration: Duration::from_secs(1),
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["total_duration"], 1000);
        assert_eq!(json["version"], "0.1.0");
    }

    #[test]
    fn all_suites_includes_registered_suites() {
        let suites = all_suites(0);
        assert_eq!(suites.len(), 4);
        assert_eq!(suites[0].name(), "Health & System");
        assert_eq!(suites[1].name(), "Entity CRUD");
        assert_eq!(suites[2].name(), "Query API");
        assert_eq!(suites[3].name(), "URL Query Parameters");
    }
}
