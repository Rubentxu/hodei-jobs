//! Health Check Traits and Types
//!
//! Provides [`HealthCheck`] trait for components to report their health status.
//! Used by [`HealthAggregator`] and [`SagaEngineWatchdog`] for monitoring.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

/// Health status of a component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Component is healthy and operating normally
    Healthy,
    /// Component is degraded but still functional
    Degraded,
    /// Component is unhealthy and not functioning
    Unhealthy,
    /// Component status is unknown
    Unknown,
}

impl HealthStatus {
    /// Check if status is healthy or degraded
    pub fn is_operational(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }

    /// Check if status is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// Metric value for health information
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum MetricValue {
    String(String),
    Int64(i64),
    Float64(f64),
    Boolean(bool),
}

impl From<String> for MetricValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<i64> for MetricValue {
    fn from(value: i64) -> Self {
        Self::Int64(value)
    }
}

impl From<f64> for MetricValue {
    fn from(value: f64) -> Self {
        Self::Float64(value)
    }
}

impl From<bool> for MetricValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

/// Detailed health information for a component
#[derive(Debug, Clone, Serialize)]
pub struct HealthInfo {
    /// Health status
    pub status: HealthStatus,
    /// Component name
    pub component: String,
    /// Last heartbeat timestamp (if available)
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Component-specific metrics
    pub metrics: HashMap<String, MetricValue>,
    /// Uptime since start
    pub uptime: Duration,
    /// Last error (if any)
    pub last_error: Option<String>,
    /// Additional context
    pub context: HashMap<String, String>,
}

impl HealthInfo {
    /// Create a new health info
    pub fn new(component: impl Into<String>, status: HealthStatus) -> Self {
        Self {
            status,
            component: component.into(),
            last_heartbeat: None,
            metrics: HashMap::new(),
            uptime: Duration::from_secs(0),
            last_error: None,
            context: HashMap::new(),
        }
    }

    /// Set last heartbeat
    pub fn with_last_heartbeat(mut self, timestamp: DateTime<Utc>) -> Self {
        self.last_heartbeat = Some(timestamp);
        self
    }

    /// Set uptime
    pub fn with_uptime(mut self, uptime: Duration) -> Self {
        self.uptime = uptime;
        self
    }

    /// Set last error
    pub fn with_last_error(mut self, error: impl Into<String>) -> Self {
        self.last_error = Some(error.into());
        self
    }

    /// Add metric
    pub fn with_metric(mut self, key: impl Into<String>, value: impl Into<MetricValue>) -> Self {
        self.metrics.insert(key.into(), value.into());
        self
    }

    /// Add context
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Check if component is healthy
    pub fn is_healthy(&self) -> bool {
        self.status.is_healthy()
    }

    /// Check if component is operational (healthy or degraded)
    pub fn is_operational(&self) -> bool {
        self.status.is_operational()
    }
}

/// Health check trait for saga engine components
///
/// Components implement this trait to provide health information
/// to watchdog system.
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Get current health status
    async fn health(&self) -> HealthInfo;

    /// Check if component is healthy (simplified)
    async fn is_healthy(&self) -> bool {
        self.health().await.is_healthy()
    }

    /// Get uptime since component started
    async fn uptime(&self) -> Duration;

    /// Get last error (if any)
    async fn last_error(&self) -> Option<String>;

    /// Perform a liveness check (is it running?)
    ///
    /// Returns true if component is running and responsive
    async fn liveness(&self) -> bool;

    /// Perform a readiness check (is it ready to process?)
    ///
    /// Returns true if component is ready to handle work
    async fn readiness(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_is_operational() {
        assert!(HealthStatus::Healthy.is_operational());
        assert!(HealthStatus::Degraded.is_operational());
        assert!(!HealthStatus::Unhealthy.is_operational());
        assert!(!HealthStatus::Unknown.is_operational());
    }

    #[test]
    fn test_health_status_is_healthy() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Degraded.is_healthy());
        assert!(!HealthStatus::Unhealthy.is_healthy());
        assert!(!HealthStatus::Unknown.is_healthy());
    }

    #[test]
    fn test_health_info_builder() {
        let health = HealthInfo::new("TestComponent", HealthStatus::Healthy)
            .with_uptime(Duration::from_secs(100))
            .with_metric("test_metric", 42i64)
            .with_context("test_key", "test_value");

        assert_eq!(health.component, "TestComponent");
        assert_eq!(health.status, HealthStatus::Healthy);
        assert_eq!(health.uptime, Duration::from_secs(100));
        assert_eq!(
            health.metrics.get("test_metric"),
            Some(&MetricValue::Int64(42))
        );
        assert_eq!(
            health.context.get("test_key"),
            Some(&"test_value".to_string())
        );
    }

    #[test]
    fn test_health_info_is_healthy() {
        let healthy = HealthInfo::new("Test", HealthStatus::Healthy);
        let degraded = HealthInfo::new("Test", HealthStatus::Degraded);
        let unhealthy = HealthInfo::new("Test", HealthStatus::Unhealthy);

        assert!(healthy.is_healthy());
        assert!(!degraded.is_healthy());
        assert!(!unhealthy.is_healthy());
    }

    #[test]
    fn test_health_info_is_operational() {
        let healthy = HealthInfo::new("Test", HealthStatus::Healthy);
        let degraded = HealthInfo::new("Test", HealthStatus::Degraded);
        let unhealthy = HealthInfo::new("Test", HealthStatus::Unhealthy);

        assert!(healthy.is_operational());
        assert!(degraded.is_operational());
        assert!(!unhealthy.is_operational());
    }
}
