//! Saga Metrics Interface
//!
//! This module defines the saga metrics interface for observability.
//! Implementations are provided by the infrastructure layer using Prometheus.

use super::SagaType;

/// Trait for saga metrics collection.
/// EPIC-45 Gap 3: Observability metrics for saga engine
///
/// Implement this trait in infrastructure layer to collect saga metrics.
/// The infrastructure crate provides a Prometheus-based implementation.
#[async_trait::async_trait]
pub trait SagaMetrics: Send + Sync {
    /// Record saga execution started
    async fn record_started(&self, saga_type: SagaType);

    /// Record saga execution completed successfully
    async fn record_completed(&self, saga_type: SagaType, duration_secs: f64);

    /// Record saga execution that required compensation
    async fn record_compensated(&self, saga_type: SagaType, compensation_step: &str);

    /// Record saga execution failed
    async fn record_failed(&self, saga_type: SagaType, error_type: &str);

    /// Record compensation step executed
    async fn record_compensation(&self, saga_type: SagaType, step_name: &str);

    /// Increment active saga count
    async fn increment_active(&self, saga_type: SagaType);

    /// Decrement active saga count
    async fn decrement_active(&self, saga_type: SagaType);

    /// Record step execution latency
    async fn record_step_duration(&self, saga_type: SagaType, step_name: &str, duration_secs: f64);
}

/// No-op metrics implementation for testing
/// EPIC-45 Gap 3: Default implementation for test environments
#[derive(Debug, Clone, Default)]
pub struct NoOpSagaMetrics;

#[async_trait::async_trait]
impl SagaMetrics for NoOpSagaMetrics {
    async fn record_started(&self, _saga_type: SagaType) {}

    async fn record_completed(&self, _saga_type: SagaType, _duration_secs: f64) {}

    async fn record_compensated(&self, _saga_type: SagaType, _compensation_step: &str) {}

    async fn record_failed(&self, _saga_type: SagaType, _error_type: &str) {}

    async fn record_compensation(&self, _saga_type: SagaType, _step_name: &str) {}

    async fn increment_active(&self, _saga_type: SagaType) {}

    async fn decrement_active(&self, _saga_type: SagaType) {}

    async fn record_step_duration(
        &self,
        _saga_type: SagaType,
        _step_name: &str,
        _duration_secs: f64,
    ) {
    }
}
