//! # saga-engine-metrics
//!
//! Prometheus metrics for saga-engine v4 workflows and activities.

use prometheus::{IntCounterVec, IntGaugeVec, Registry, histogram_opts, opts};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Prometheus metrics implementation for saga-engine
#[derive(Clone)]
pub struct PrometheusMetrics {
    /// Prometheus registry
    registry: Registry,

    // Workflow metrics
    workflow_started: IntCounterVec,
    workflow_completed: IntCounterVec,
    workflow_failed: IntCounterVec,
    workflow_duration: prometheus::HistogramVec,
    workflow_active: IntGaugeVec,

    // Activity metrics
    activity_started: IntCounterVec,
    activity_completed: IntCounterVec,
    activity_failed: IntCounterVec,
    activity_duration: prometheus::HistogramVec,
}

impl PrometheusMetrics {
    /// Create new Prometheus metrics
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // Workflow metrics
        let workflow_started = IntCounterVec::new(
            opts!(
                "saga_workflow_started_total",
                "Total workflows started across all types"
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_started.clone()))?;

        let workflow_completed = IntCounterVec::new(
            opts!(
                "saga_workflow_completed_total",
                "Total workflows completed successfully"
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_completed.clone()))?;

        let workflow_failed = IntCounterVec::new(
            opts!("saga_workflow_failed_total", "Total workflows that failed"),
            &["workflow_type", "error_type"],
        )?;
        registry.register(Box::new(workflow_failed.clone()))?;

        let workflow_duration = prometheus::HistogramVec::new(
            histogram_opts!(
                "saga_workflow_duration_seconds",
                "Workflow execution duration in seconds",
                vec![0.1, 0.5, 1.0, 5.0, 30.0, 60.0, 300.0, 1800.0]
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_duration.clone()))?;

        let workflow_active = IntGaugeVec::new(
            opts!("saga_workflow_active", "Currently active workflows"),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_active.clone()))?;

        // Activity metrics
        let activity_started = IntCounterVec::new(
            opts!("saga_activity_started_total", "Total activities started"),
            &["activity_type"],
        )?;
        registry.register(Box::new(activity_started.clone()))?;

        let activity_completed = IntCounterVec::new(
            opts!(
                "saga_activity_completed_total",
                "Total activities completed successfully"
            ),
            &["activity_type"],
        )?;
        registry.register(Box::new(activity_completed.clone()))?;

        let activity_failed = IntCounterVec::new(
            opts!("saga_activity_failed_total", "Total activities that failed"),
            &["activity_type", "error_type"],
        )?;
        registry.register(Box::new(activity_failed.clone()))?;

        let activity_duration = prometheus::HistogramVec::new(
            histogram_opts!(
                "saga_activity_duration_seconds",
                "Activity execution duration in seconds",
                vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 30.0]
            ),
            &["activity_type"],
        )?;
        registry.register(Box::new(activity_duration.clone()))?;

        Ok(Self {
            registry,
            workflow_started,
            workflow_completed,
            workflow_failed,
            workflow_duration,
            workflow_active,
            activity_started,
            activity_completed,
            activity_failed,
            activity_duration,
        })
    }

    /// Get the registry for custom metric registration
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

impl SagaMetrics for PrometheusMetrics {
    fn workflow_started(&self, workflow_type: &str) {
        self.workflow_started
            .with_label_values(&[workflow_type])
            .inc();
        self.workflow_active
            .with_label_values(&[workflow_type])
            .inc();
    }

    fn workflow_completed(&self, workflow_type: &str, duration_ms: u64) {
        self.workflow_completed
            .with_label_values(&[workflow_type])
            .inc();
        self.workflow_duration
            .with_label_values(&[workflow_type])
            .observe(duration_ms as f64 / 1000.0);
        self.workflow_active
            .with_label_values(&[workflow_type])
            .dec();
    }

    fn workflow_failed(&self, workflow_type: &str, error_type: &str) {
        self.workflow_failed
            .with_label_values(&[workflow_type, error_type])
            .inc();
        self.workflow_active
            .with_label_values(&[workflow_type])
            .dec();
    }

    fn activity_started(&self, activity_type: &str) {
        self.activity_started
            .with_label_values(&[activity_type])
            .inc();
    }

    fn activity_completed(&self, activity_type: &str, duration_ms: u64) {
        self.activity_completed
            .with_label_values(&[activity_type])
            .inc();
        self.activity_duration
            .with_label_values(&[activity_type])
            .observe(duration_ms as f64 / 1000.0);
    }

    fn activity_failed(&self, activity_type: &str, error_type: &str) {
        self.activity_failed
            .with_label_values(&[activity_type, error_type])
            .inc();
    }
}

/// Trait for saga metrics collection
pub trait SagaMetrics: Send + Sync + 'static {
    fn workflow_started(&self, workflow_type: &str);
    fn workflow_completed(&self, workflow_type: &str, duration_ms: u64);
    fn workflow_failed(&self, workflow_type: &str, error_type: &str);
    fn activity_started(&self, activity_type: &str);
    fn activity_completed(&self, activity_type: &str, duration_ms: u64);
    fn activity_failed(&self, activity_type: &str, error_type: &str);
}

/// No-op metrics for testing
#[derive(Debug, Default, Clone)]
pub struct NoopSagaMetrics;

impl SagaMetrics for NoopSagaMetrics {
    fn workflow_started(&self, _workflow_type: &str) {}
    fn workflow_completed(&self, _workflow_type: &str, _duration_ms: u64) {}
    fn workflow_failed(&self, _workflow_type: &str, _error_type: &str) {}
    fn activity_started(&self, _activity_type: &str) {}
    fn activity_completed(&self, _activity_type: &str, _duration_ms: u64) {}
    fn activity_failed(&self, _activity_type: &str, _error_type: &str) {}
}

/// Wrapper for optional metrics
#[derive(Clone, Default)]
pub struct OptionalMetrics(pub Option<Arc<dyn SagaMetrics>>);

impl OptionalMetrics {
    pub fn new(metrics: Option<Arc<dyn SagaMetrics>>) -> Self {
        Self(metrics)
    }

    pub fn workflow_started(&self, workflow_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.workflow_started(workflow_type);
        }
    }

    pub fn workflow_completed(&self, workflow_type: &str, duration_ms: u64) {
        if let Some(metrics) = &self.0 {
            metrics.workflow_completed(workflow_type, duration_ms);
        }
    }

    pub fn workflow_failed(&self, workflow_type: &str, error_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.workflow_failed(workflow_type, error_type);
        }
    }

    pub fn activity_started(&self, activity_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.activity_started(activity_type);
        }
    }

    pub fn activity_completed(&self, activity_type: &str, duration_ms: u64) {
        if let Some(metrics) = &self.0 {
            metrics.activity_completed(activity_type, duration_ms);
        }
    }

    pub fn activity_failed(&self, activity_type: &str, error_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.activity_failed(activity_type, error_type);
        }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub path: String,
    pub workflow_duration_buckets: Vec<f64>,
    pub activity_duration_buckets: Vec<f64>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/metrics".to_string(),
            workflow_duration_buckets: vec![0.1, 0.5, 1.0, 5.0, 30.0, 60.0, 300.0, 1800.0],
            activity_duration_buckets: vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 30.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_metrics_creation() {
        let metrics = PrometheusMetrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_noop_metrics() {
        let metrics = NoopSagaMetrics::default();
        metrics.workflow_started("test");
        metrics.workflow_completed("test", 100);
        metrics.workflow_failed("test", "error");
        metrics.activity_started("test");
        metrics.activity_completed("test", 50);
        metrics.activity_failed("test", "error");
    }

    #[test]
    fn test_optional_metrics() {
        let noop: Arc<dyn SagaMetrics> = Arc::new(NoopSagaMetrics::default());
        let metrics = OptionalMetrics::new(Some(noop));
        metrics.workflow_started("test");
        metrics.activity_completed("test", 100);

        let empty = OptionalMetrics::new(None);
        empty.workflow_started("test");
    }
}
