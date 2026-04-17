use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;

use crate::client::ElysianClient;

mod acl;
mod auth;
mod crud;
mod health;
mod kv;
mod nested;
mod query;
mod query_params;
mod schema;
mod tcp;
mod transactions;

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

// ---- TestResult builders ---------------------------------------------------
//
// Shared helpers used by every suite. The `request` argument is a
// human-readable summary of the request for the final report — it is NOT
// the wire-level URL or body. URL-parameter-form strings like
// `"GET /api/x?filter[foo][eq]=bar"` appear verbatim in reports; reqwest /
// url handle percent-encoding for the actual HTTP call.

pub(crate) fn pass(
    suite: &str,
    name: &str,
    request: String,
    status: Option<u16>,
    duration: Duration,
) -> TestResult {
    TestResult {
        suite: suite.to_string(),
        name: name.to_string(),
        status: TestStatus::Passed,
        duration,
        error: None,
        request: Some(request),
        response_status: status,
    }
}

pub(crate) fn fail(
    suite: &str,
    name: &str,
    request: String,
    status: Option<u16>,
    duration: Duration,
    error: impl Into<String>,
) -> TestResult {
    TestResult {
        suite: suite.to_string(),
        name: name.to_string(),
        status: TestStatus::Failed,
        duration,
        error: Some(error.into()),
        request: Some(request),
        response_status: status,
    }
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
    "battle_schema_auto",
    "battle_schema_manual",
    "battle_typed",
    "battle_typed2",
    "battle_auth_data",
    "battle_acl_data",
    "battle_tx_items",
    "battle_export_test",
    "battle_hooked_entity",
    "battle_posts",
    "battle_authors_nested",
    "battle_jobs_nested",
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
pub fn all_suites(tcp_port: u16) -> Vec<Box<dyn TestSuite>> {
    vec![
        Box::new(health::HealthSuite),
        Box::new(crud::CrudSuite),
        Box::new(query::QuerySuite),
        Box::new(query_params::QueryParamsSuite),
        Box::new(nested::NestedSuite),
        Box::new(schema::SchemaSuite),
        Box::new(auth::AuthSuite),
        Box::new(acl::AclSuite),
        Box::new(transactions::TransactionsSuite),
        Box::new(kv::KvSuite),
        Box::new(tcp::TcpSuite::new(tcp_port)),
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
        // Non-zero port because TcpSuite::new debug_asserts against 0
        // (see its docs) — any non-zero value works for this static
        // registration check.
        let suites = all_suites(1);
        assert_eq!(suites.len(), 11);
        assert_eq!(suites[0].name(), "Health & System");
        assert_eq!(suites[1].name(), "Entity CRUD");
        assert_eq!(suites[2].name(), "Query API");
        assert_eq!(suites[3].name(), "URL Query Parameters");
        assert_eq!(suites[4].name(), "Nested Entities");
        assert_eq!(suites[5].name(), "Schema");
        assert_eq!(suites[6].name(), "Authentication");
        assert_eq!(suites[7].name(), "ACL");
        assert_eq!(suites[8].name(), "Transactions");
        assert_eq!(suites[9].name(), "KV Store");
        assert_eq!(suites[10].name(), "TCP Protocol");
    }
}
