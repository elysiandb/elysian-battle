use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;

use crate::client::ElysianClient;
use crate::suites::{TestResult, TestStatus, TestSuite};

pub struct HealthSuite;

#[async_trait]
impl TestSuite for HealthSuite {
    fn name(&self) -> &'static str {
        "Health & System"
    }

    fn description(&self) -> &'static str {
        "Validates system endpoints: health, stats, config, save, version header"
    }

    async fn setup(&self, _client: &ElysianClient) -> Result<()> {
        Ok(())
    }

    async fn run(&self, client: &ElysianClient) -> Vec<TestResult> {
        let suite = self.name().to_string();
        let mut results = Vec::with_capacity(5);

        // H-01 — Health endpoint returns 200
        results.push(h01_health_returns_200(&suite, client).await);

        // H-02 — Stats returns valid JSON
        results.push(h02_stats_returns_valid_json(&suite, client).await);

        // H-03 — Config returns current config
        results.push(h03_config_returns_current(&suite, client).await);

        // H-04 — Force save succeeds
        results.push(h04_force_save_succeeds(&suite, client).await);

        // H-05 — Version header present
        results.push(h05_version_header_present(&suite, client).await);

        results
    }

    async fn teardown(&self, _client: &ElysianClient) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Individual test functions
// ---------------------------------------------------------------------------

async fn h01_health_returns_200(suite: &str, client: &ElysianClient) -> TestResult {
    let start = Instant::now();
    let request = "GET /health".to_string();

    match client.health().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let duration = start.elapsed();
            if status == 200 {
                TestResult {
                    suite: suite.to_string(),
                    name: "H-01 Health endpoint returns 200".to_string(),
                    status: TestStatus::Passed,
                    duration,
                    error: None,
                    request: Some(request),
                    response_status: Some(status),
                }
            } else {
                TestResult {
                    suite: suite.to_string(),
                    name: "H-01 Health endpoint returns 200".to_string(),
                    status: TestStatus::Failed,
                    duration,
                    error: Some(format!("expected status 200, got {status}")),
                    request: Some(request),
                    response_status: Some(status),
                }
            }
        }
        Err(e) => TestResult {
            suite: suite.to_string(),
            name: "H-01 Health endpoint returns 200".to_string(),
            status: TestStatus::Failed,
            duration: start.elapsed(),
            error: Some(format!("request failed: {e:#}")),
            request: Some(request),
            response_status: None,
        },
    }
}

async fn h02_stats_returns_valid_json(suite: &str, client: &ElysianClient) -> TestResult {
    let start = Instant::now();
    let request = "GET /stats".to_string();

    match client.stats().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status != 200 {
                return TestResult {
                    suite: suite.to_string(),
                    name: "H-02 Stats returns valid JSON".to_string(),
                    status: TestStatus::Failed,
                    duration: start.elapsed(),
                    error: Some(format!("expected status 200, got {status}")),
                    request: Some(request),
                    response_status: Some(status),
                };
            }

            match resp.json::<serde_json::Value>().await {
                Ok(body) => {
                    let duration = start.elapsed();
                    let missing: Vec<&str> = ["keys_count", "uptime_seconds", "total_requests"]
                        .iter()
                        .filter(|k| body.get(**k).is_none())
                        .copied()
                        .collect();

                    if missing.is_empty() {
                        TestResult {
                            suite: suite.to_string(),
                            name: "H-02 Stats returns valid JSON".to_string(),
                            status: TestStatus::Passed,
                            duration,
                            error: None,
                            request: Some(request),
                            response_status: Some(status),
                        }
                    } else {
                        TestResult {
                            suite: suite.to_string(),
                            name: "H-02 Stats returns valid JSON".to_string(),
                            status: TestStatus::Failed,
                            duration,
                            error: Some(format!("missing fields: {}", missing.join(", "))),
                            request: Some(request),
                            response_status: Some(status),
                        }
                    }
                }
                Err(e) => TestResult {
                    suite: suite.to_string(),
                    name: "H-02 Stats returns valid JSON".to_string(),
                    status: TestStatus::Failed,
                    duration: start.elapsed(),
                    error: Some(format!("failed to parse JSON: {e:#}")),
                    request: Some(request),
                    response_status: Some(status),
                },
            }
        }
        Err(e) => TestResult {
            suite: suite.to_string(),
            name: "H-02 Stats returns valid JSON".to_string(),
            status: TestStatus::Failed,
            duration: start.elapsed(),
            error: Some(format!("request failed: {e:#}")),
            request: Some(request),
            response_status: None,
        },
    }
}

async fn h03_config_returns_current(suite: &str, client: &ElysianClient) -> TestResult {
    let start = Instant::now();
    let request = "GET /config".to_string();

    match client.config().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status != 200 {
                return TestResult {
                    suite: suite.to_string(),
                    name: "H-03 Config returns current config".to_string(),
                    status: TestStatus::Failed,
                    duration: start.elapsed(),
                    error: Some(format!("expected status 200, got {status}")),
                    request: Some(request),
                    response_status: Some(status),
                };
            }

            match resp.json::<serde_json::Value>().await {
                Ok(body) => {
                    let duration = start.elapsed();
                    let mut errors = Vec::new();

                    // Check Engine.Name == "internal" (ElysianDB uses PascalCase keys)
                    match body.pointer("/Engine/Name").and_then(|v| v.as_str()) {
                        Some("internal") => {}
                        Some(other) => {
                            errors.push(format!(
                                "Engine.Name: expected \"internal\", got \"{other}\""
                            ));
                        }
                        None => errors.push("Engine.Name field missing".to_string()),
                    }

                    // Check Server.HTTP.Port exists (numeric)
                    if body
                        .pointer("/Server/HTTP/Port")
                        .and_then(|v| v.as_u64())
                        .is_none()
                    {
                        errors.push("Server.HTTP.Port missing or not a number".to_string());
                    }

                    // Check Server.TCP.Port exists (numeric)
                    if body
                        .pointer("/Server/TCP/Port")
                        .and_then(|v| v.as_u64())
                        .is_none()
                    {
                        errors.push("Server.TCP.Port missing or not a number".to_string());
                    }

                    if errors.is_empty() {
                        TestResult {
                            suite: suite.to_string(),
                            name: "H-03 Config returns current config".to_string(),
                            status: TestStatus::Passed,
                            duration,
                            error: None,
                            request: Some(request),
                            response_status: Some(status),
                        }
                    } else {
                        TestResult {
                            suite: suite.to_string(),
                            name: "H-03 Config returns current config".to_string(),
                            status: TestStatus::Failed,
                            duration,
                            error: Some(errors.join("; ")),
                            request: Some(request),
                            response_status: Some(status),
                        }
                    }
                }
                Err(e) => TestResult {
                    suite: suite.to_string(),
                    name: "H-03 Config returns current config".to_string(),
                    status: TestStatus::Failed,
                    duration: start.elapsed(),
                    error: Some(format!("failed to parse JSON: {e:#}")),
                    request: Some(request),
                    response_status: Some(status),
                },
            }
        }
        Err(e) => TestResult {
            suite: suite.to_string(),
            name: "H-03 Config returns current config".to_string(),
            status: TestStatus::Failed,
            duration: start.elapsed(),
            error: Some(format!("request failed: {e:#}")),
            request: Some(request),
            response_status: None,
        },
    }
}

async fn h04_force_save_succeeds(suite: &str, client: &ElysianClient) -> TestResult {
    let start = Instant::now();
    let request = "POST /save".to_string();

    match client.save().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let duration = start.elapsed();
            if status == 200 || status == 204 {
                TestResult {
                    suite: suite.to_string(),
                    name: "H-04 Force save succeeds".to_string(),
                    status: TestStatus::Passed,
                    duration,
                    error: None,
                    request: Some(request),
                    response_status: Some(status),
                }
            } else {
                TestResult {
                    suite: suite.to_string(),
                    name: "H-04 Force save succeeds".to_string(),
                    status: TestStatus::Failed,
                    duration,
                    error: Some(format!("expected status 200 or 204, got {status}")),
                    request: Some(request),
                    response_status: Some(status),
                }
            }
        }
        Err(e) => TestResult {
            suite: suite.to_string(),
            name: "H-04 Force save succeeds".to_string(),
            status: TestStatus::Failed,
            duration: start.elapsed(),
            error: Some(format!("request failed: {e:#}")),
            request: Some(request),
            response_status: None,
        },
    }
}

async fn h05_version_header_present(suite: &str, client: &ElysianClient) -> TestResult {
    let start = Instant::now();
    let request = "GET /health".to_string();

    match client.health().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let duration = start.elapsed();

            match resp.headers().get("X-Elysian-Version") {
                Some(val) => {
                    if val.is_empty() {
                        TestResult {
                            suite: suite.to_string(),
                            name: "H-05 Version header present".to_string(),
                            status: TestStatus::Failed,
                            duration,
                            error: Some("X-Elysian-Version header present but empty".to_string()),
                            request: Some(request),
                            response_status: Some(status),
                        }
                    } else {
                        TestResult {
                            suite: suite.to_string(),
                            name: "H-05 Version header present".to_string(),
                            status: TestStatus::Passed,
                            duration,
                            error: None,
                            request: Some(request),
                            response_status: Some(status),
                        }
                    }
                }
                None => TestResult {
                    suite: suite.to_string(),
                    name: "H-05 Version header present".to_string(),
                    status: TestStatus::Failed,
                    duration,
                    error: Some("X-Elysian-Version header missing".to_string()),
                    request: Some(request),
                    response_status: Some(status),
                },
            }
        }
        Err(e) => TestResult {
            suite: suite.to_string(),
            name: "H-05 Version header present".to_string(),
            status: TestStatus::Failed,
            duration: start.elapsed(),
            error: Some(format!("request failed: {e:#}")),
            request: Some(request),
            response_status: None,
        },
    }
}
