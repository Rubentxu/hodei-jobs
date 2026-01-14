//! Prometheus-based Saga Metrics Implementation
//!
//! This module provides a Prometheus metrics implementation for saga observability,
//! exposing saga execution metrics for monitoring and alerting.
//!
//! # Metrics Exposed
//!
//! - `hodei_saga_started_total{saga_type}` - Total sagas started
//! - `hodei_saga_completed_duration_seconds{saga_type}` - Saga completion duration
//! - `hodei_saga_compensated_total{saga_type}` - Total sagas requiring compensation
//! - `hodei_saga_failed_total{saga_type,error_type}` - Total saga failures
//! - `hodei_saga_step_duration_seconds{saga_type,step_name}` - Step execution duration
//! - `hodei_saga_active_current{saga_type}` - Currently active sagas
//! - `hodei_saga_version_conflicts_total{saga_type}` - Version conflicts detected
//! - `hodei_saga_retries_total{saga_type,reason}` - Retries due to conflicts

use crate::telemetry::PrometheusExporter;
use hodei_server_domain::saga::{SagaMetrics as SagaMetricsTrait, SagaType};
use prometheus_client::{
    metrics::{counter::Counter, gauge::Gauge, histogram::Histogram},
    registry::Registry,
};
use std::sync::Arc;
use std::time::Duration;

/// Prometheus-based saga metrics implementation
#[derive(Debug, Clone)]
pub struct PrometheusSagaMetrics {
    started_total: Arc<Counter>,
    completed_duration: Arc<Histogram>,
    compensated_total: Arc<Counter>,
    failed_total: Arc<Counter>,
    step_duration: Arc<Histogram>,
    active_current: Arc<Gauge>,
    version_conflicts_total: Arc<Counter>,
    retries_total: Arc<Counter>,
}

impl PrometheusSagaMetrics {
    /// Create new Prometheus saga metrics
    pub fn new() -> Self {
        Self {
            started_total: Arc::new(Counter::new()),
            completed_duration: Arc::new(Histogram::new(
                prometheus_exponential_buckets(0.005, 2.0, 10).unwrap(),
            )),
            compensated_total: Arc::new(Counter::new()),
            failed_total: Arc::new(Counter::new()),
            step_duration: Arc::new(Histogram::new(
                prometheus_exponential_buckets(0.001, 2.0, 10).unwrap(),
            )),
            active_current: Arc::new(Gauge::new()),
            version_conflicts_total: Arc::new(Counter::new()),
            retries_total: Arc::new(Counter::new()),
        }
    }

    /// Register metrics with a Prometheus registry
    pub fn register(&self, registry: &mut Registry) {
        let saga_type_labels = ["saga_type"];
        let error_type_labels = ["saga_type", "error_type"];
        let step_name_labels = ["saga_type", "step_name"];
        let reason_labels = ["saga_type", "reason"];

        registry.register(
            "hodei_saga_started_total",
            "Total number of sagas started",
            self.started_total.clone(),
            saga_type_labels,
        );

        registry.register(
            "hodei_saga_completed_duration_seconds",
            "Saga completion duration in seconds",
            self.completed_duration.clone(),
            saga_type_labels,
        );

        registry.register(
            "hodei_saga_compensated_total",
            "Total number of sagas requiring compensation",
            self.compensated_total.clone(),
            saga_type_labels,
        );

        registry.register(
            "hodei_saga_failed_total",
            "Total number of failed sagas",
            self.failed_total.clone(),
            error_type_labels,
        );

        registry.register(
            "hodei_saga_step_duration_seconds",
            "Step execution duration in seconds",
            self.step_duration.clone(),
            step_name_labels,
        );

        registry.register(
            "hodei_saga_active_current",
            "Current number of active sagas",
            self.active_current.clone(),
            saga_type_labels,
        );

        registry.register(
            "hodei_saga_version_conflicts_total",
            "Total number of version conflicts",
            self.version_conflicts_total.clone(),
            saga_type_labels,
        );

        registry.register(
            "hodei_saga_retries_total",
            "Total number of retries",
            self.retries_total.clone(),
            reason_labels,
        );
    }

    /// Get the saga type label value
    fn saga_type_label(saga_type: &SagaType) -> &'static str {
        match saga_type {
            SagaType::Execution => "execution",
            SagaType::Provisioning => "provisioning",
            SagaType::Recovery => "recovery",
            SagaType::Cancellation => "cancellation",
            SagaType::Timeout => "timeout",
            SagaType::Cleanup => "cleanup",
        }
    }
}

impl Default for PrometheusSagaMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SagaMetricsTrait for PrometheusSagaMetrics {
    async fn record_saga_started(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(&saga_type);
        self.started_total.with_label_values(&[label]).inc();
        self.active_current.with_label_values(&[label]).inc();
    }

    async fn record_saga_completed(&self, saga_type: SagaType, duration: Duration) {
        let label = Self::saga_type_label(&saga_type);
        self.completed_duration
            .with_label_values(&[label])
            .observe(duration.as_secs_f64());
        self.active_current.with_label_values(&[label]).dec();
    }

    async fn record_saga_compensated(&self, saga_type: SagaType, duration: Duration) {
        let label = Self::saga_type_label(&saga_type);
        self.compensated_total.with_label_values(&[label]).inc();
        self.completed_duration
            .with_label_values(&[label])
            .observe(duration.as_secs_f64());
        self.active_current.with_label_values(&[label]).dec();
    }

    async fn record_saga_failed(&self, saga_type: SagaType, error_type: &str) {
        let label = Self::saga_type_label(&saga_type);
        self.failed_total
            .with_label_values(&[label, error_type])
            .inc();
        self.active_current.with_label_values(&[label]).dec();
    }

    async fn record_step_duration(&self, saga_type: SagaType, step_name: &str, duration: Duration) {
        let saga_label = Self::saga_type_label(&saga_type);
        self.step_duration
            .with_label_values(&[saga_label, step_name])
            .observe(duration.as_secs_f64());
    }

    async fn increment_active(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(&saga_type);
        self.active_current.with_label_values(&[label]).inc();
    }

    async fn decrement_active(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(&saga_type);
        self.active_current.with_label_values(&[label]).dec();
    }

    async fn record_version_conflict(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(&saga_type);
        self.version_conflicts_total
            .with_label_values(&[label])
            .inc();
    }

    async fn record_retry(&self, saga_type: SagaType, reason: &str) {
        let label = Self::saga_type_label(&saga_type);
        self.retries_total.with_label_values(&[label, reason]).inc();
    }
}

/// Create exponential buckets for histogram
fn prometheus_exponential_buckets(
    start: f64,
    factor: f64,
    count: usize,
) -> Result<Vec<f64>, std::num::ParseFloatError> {
    let mut buckets = Vec::with_capacity(count);
    let mut current = start;
    for _ in 0..count {
        buckets.push(current);
        current *= factor;
    }
    Ok(buckets)
}

/// Wrapper for using saga metrics with Prometheus exporter
#[derive(Debug, Clone)]
pub struct SagaMetricsExporter {
    metrics: Arc<PrometheusSagaMetrics>,
    registry: Arc<tokio::sync::Mutex<Registry>>,
}

impl SagaMetricsExporter {
    /// Create a new saga metrics exporter
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let metrics = Arc::new(PrometheusSagaMetrics::new());
        metrics.register(&mut registry);

        Self {
            metrics,
            registry: Arc::new(tokio::sync::Mutex::new(registry)),
        }
    }

    /// Get reference to metrics
    pub fn metrics(&self) -> &PrometheusSagaMetrics {
        &self.metrics
    }

    /// Get registry for HTTP exposition
    pub async fn registry(&self) -> tokio::sync::MutexGuard<'_, Registry> {
        self.registry.lock().await
    }

    /// Get the inner metrics as Arc
    pub fn inner(&self) -> Arc<PrometheusSagaMetrics> {
        self.metrics.clone()
    }
}

impl Default for SagaMetricsExporter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::SagaType;

    #[tokio::test]
    async fn test_prometheus_metrics_record_started() {
        let metrics = PrometheusSagaMetrics::new();
        metrics.record_saga_started(SagaType::Execution).await;
        // Would need Prometheus registry to verify
    }

    #[tokio::test]
    async fn test_prometheus_metrics_record_completed() {
        let metrics = PrometheusSagaMetrics::new();
        metrics
            .record_saga_completed(SagaType::Execution, Duration::from_secs(5))
            .await;
    }

    #[tokio::test]
    async fn test_prometheus_metrics_record_failed() {
        let metrics = PrometheusSagaMetrics::new();
        metrics
            .record_saga_failed(SagaType::Execution, "timeout")
            .await;
    }

    #[tokio::test]
    async fn test_prometheus_metrics_record_step() {
        let metrics = PrometheusSagaMetrics::new();
        metrics
            .record_step_duration(SagaType::Execution, "step1", Duration::from_millis(100))
            .await;
    }

    #[tokio::test]
    async fn test_saga_metrics_exporter() {
        let exporter = SagaMetricsExporter::new();
        assert!(exporter.metrics().started_total.len() > 0);
    }

    #[test]
    fn test_exponential_buckets() {
        let buckets = prometheus_exponential_buckets(0.001, 2.0, 5).unwrap();
        assert_eq!(buckets.len(), 5);
        assert_eq!(buckets[0], 0.001);
        assert_eq!(buckets[1], 0.002);
        assert_eq!(buckets[2], 0.004);
    }

    #[test]
    fn test_saga_type_label() {
        assert_eq!(
            PrometheusSagaMetrics::saga_type_label(&SagaType::Execution),
            "execution"
        );
        assert_eq!(
            PrometheusSagaMetrics::saga_type_label(&SagaType::Provisioning),
            "provisioning"
        );
        assert_eq!(
            PrometheusSagaMetrics::saga_type_label(&SagaType::Recovery),
            "recovery"
        );
        assert_eq!(
            PrometheusSagaMetrics::saga_type_label(&SagaType::Cancellation),
            "cancellation"
        );
        assert_eq!(
            PrometheusSagaMetrics::saga_type_label(&SagaType::Timeout),
            "timeout"
        );
        assert_eq!(
            PrometheusSagaMetrics::saga_type_label(&SagaType::Cleanup),
            "cleanup"
        );
    }
}
