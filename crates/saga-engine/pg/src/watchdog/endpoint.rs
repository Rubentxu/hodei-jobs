//! Health Endpoints
//!
//! HTTP endpoints for Kubernetes liveness/readiness probes
//! and general health monitoring.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

use super::aggregator::{HealthAggregator, ReadinessCheck};

/// Overall health response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Overall health status
    pub status: String,
    /// Timestamp of health check
    pub timestamp: DateTime<Utc>,
    /// Health status per component
    pub components: Vec<ComponentHealth>,
    /// Overall uptime
    pub uptime_seconds: u64,
}

/// Component health information
#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Health status
    pub status: String,
    /// Last heartbeat timestamp
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Component-specific metrics
    pub metrics: HashMap<String, serde_json::Value>,
    /// Last error (if any)
    pub last_error: Option<String>,
}

/// Readiness probe response
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    /// Whether system is ready
    pub ready: bool,
    /// Individual readiness checks
    pub checks: Vec<ReadinessCheckResponse>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Readiness check response
#[derive(Debug, Serialize)]
pub struct ReadinessCheckResponse {
    /// Check name
    pub name: String,
    /// Check description
    pub description: String,
    /// Whether check passed
    pub passed: bool,
}

/// Liveness probe response
#[derive(Debug, Serialize)]
pub struct LivenessResponse {
    /// Whether system is alive
    pub alive: bool,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Health endpoint for Kubernetes probes
pub struct HealthEndpoint {
    /// Health aggregator (cached data from watchdog)
    aggregator: Arc<HealthAggregator>,
}

impl HealthEndpoint {
    /// Create new health endpoint
    pub fn new(aggregator: Arc<HealthAggregator>) -> Self {
        Self { aggregator }
    }

    /// GET /health - Overall health check
    ///
    /// Returns overall health status and per-component health
    pub async fn get_health(&self) -> HealthResponse {
        let overall_status = self.aggregator.get_overall_status().await;
        let components = self.aggregator.get_all_components().await;
        let timestamp = Utc::now();

        // Calculate overall uptime (simplified)
        let uptime_seconds = if let Some(last_update) = self.aggregator.get_last_update().await {
            let elapsed = timestamp.signed_duration_since(last_update);
            elapsed.num_seconds().max(0) as u64
        } else {
            0
        };

        let component_healths: Vec<ComponentHealth> = components
            .into_iter()
            .map(|health| {
                let metrics = health
                    .metrics
                    .into_iter()
                    .map(|(k, v)| {
                        let json_value = match v {
                            super::super::health_check::MetricValue::String(s) => {
                                serde_json::json!(s)
                            }
                            super::super::health_check::MetricValue::Int64(i) => {
                                serde_json::json!(i)
                            }
                            super::super::health_check::MetricValue::Float64(f) => {
                                serde_json::json!(f)
                            }
                            super::super::health_check::MetricValue::Boolean(b) => {
                                serde_json::json!(b)
                            }
                        };
                        (k, json_value)
                    })
                    .collect();

                ComponentHealth {
                    name: health.component,
                    status: format!("{:?}", health.status).to_lowercase(),
                    last_heartbeat: health.last_heartbeat,
                    metrics,
                    last_error: health.last_error,
                }
            })
            .collect();

        HealthResponse {
            status: format!("{:?}", overall_status).to_lowercase(),
            timestamp,
            components: component_healths,
            uptime_seconds,
        }
    }

    /// GET /health/ready - Readiness probe
    ///
    /// Used by Kubernetes to determine if pod is ready to receive traffic.
    /// Returns true if all readiness checks passed.
    pub async fn get_readiness(&self) -> ReadinessResponse {
        let ready = self.aggregator.is_ready().await;
        let checks = self.aggregator.get_readiness_checks().await;
        let timestamp = Utc::now();

        let check_responses: Vec<ReadinessCheckResponse> = checks
            .into_iter()
            .map(|check| ReadinessCheckResponse {
                name: check.name,
                description: check.description,
                passed: check.passed,
            })
            .collect();

        ReadinessResponse {
            ready,
            checks: check_responses,
            timestamp,
        }
    }

    /// GET /health/live - Liveness probe
    ///
    /// Used by Kubernetes to determine if pod is alive.
    /// Returns true if watchdog is running and monitoring components.
    pub async fn get_liveness(&self) -> LivenessResponse {
        let alive = self.aggregator.is_operational().await;
        let timestamp = Utc::now();

        LivenessResponse { alive, timestamp }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watchdog::health_check::HealthStatus;

    #[tokio::test]
    async fn test_health_endpoint_get_health() {
        let aggregator = Arc::new(HealthAggregator::new());
        let endpoint = HealthEndpoint::new(aggregator.clone());

        // Add some health data
        aggregator
            .update_component_health(
                "Component1".to_string(),
                crate::watchdog::health_check::HealthInfo::new("Component1", HealthStatus::Healthy),
            )
            .await;

        let response = endpoint.get_health().await;

        assert_eq!(response.status, "healthy");
        assert!(!response.components.is_empty());
        assert_eq!(response.components.len(), 1);
        assert_eq!(response.components[0].name, "Component1");
    }

    #[tokio::test]
    async fn test_health_endpoint_get_readiness() {
        let aggregator = Arc::new(HealthAggregator::new());
        let endpoint = HealthEndpoint::new(aggregator.clone());

        // No checks -> ready
        let response = endpoint.get_readiness().await;
        assert!(response.ready);
        assert!(response.checks.is_empty());

        // Add failing check
        aggregator
            .update_readiness_check(ReadinessCheck {
                name: "Check1".to_string(),
                description: "Test check".to_string(),
                passed: false,
            })
            .await;

        let response = endpoint.get_readiness().await;
        assert!(!response.ready);
        assert_eq!(response.checks.len(), 1);
        assert!(!response.checks[0].passed);
    }

    #[tokio::test]
    async fn test_health_endpoint_get_liveness() {
        let aggregator = Arc::new(HealthAggregator::new());
        let endpoint = HealthEndpoint::new(aggregator.clone());

        // No data -> operational
        let response = endpoint.get_liveness().await;
        assert!(response.alive);

        // Add unhealthy component
        aggregator
            .update_component_health(
                "Component1".to_string(),
                crate::watchdog::health_check::HealthInfo::new(
                    "Component1",
                    HealthStatus::Unhealthy,
                ),
            )
            .await;

        let response = endpoint.get_liveness().await;
        assert!(!response.alive);
    }
}
