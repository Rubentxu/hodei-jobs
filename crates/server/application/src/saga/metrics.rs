//! Saga Metrics Implementation - Application Layer
//!
//! Provides saga metrics implementations for the application layer.
//! This module re-exports and provides convenient access to saga metrics
//! using the infrastructure's Prometheus implementation.
//!
//! EPIC-46 Section 9.1: Saga Metrics for observability

pub use hodei_server_domain::saga::{SagaMetrics, SagaType};

/// Alias for the domain's SagaMetrics trait
pub type SagaMetricsTrait = dyn SagaMetrics + Send + Sync;

/// Prometheus-based saga metrics implementation.
///
/// This is a wrapper around the infrastructure's PrometheusSagaMetrics
/// that provides the domain's SagaMetrics trait implementation.
pub type PrometheusSagaMetrics = hodei_server_infrastructure::metrics::PrometheusSagaMetrics;

/// Create a new Prometheus-based saga metrics instance.
///
/// # Arguments
/// * `registry` - The Prometheus registry to use
///
/// # Returns
/// A new PrometheusSagaMetrics instance
pub fn create_prometheus_metrics(
    registry: &prometheus::Registry,
) -> Result<PrometheusSagaMetrics, prometheus::Error> {
    PrometheusSagaMetrics::new(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::SagaMetrics;

    #[tokio::test]
    async fn test_saga_metrics_trait_object() {
        // Test that we can create a trait object from the infrastructure implementation
        let registry = prometheus::Registry::new();
        let metrics = create_prometheus_metrics(&registry).unwrap();

        // Should be able to use as dyn SagaMetrics
        let _trait_object: Box<dyn SagaMetrics + Send + Sync> = Box::new(metrics);
    }

    #[tokio::test]
    async fn test_saga_type_all_variants() {
        // Test all saga types can be used with metrics
        let registry = prometheus::Registry::new();
        let metrics = create_prometheus_metrics(&registry).unwrap();

        let trait_metrics: &dyn SagaMetrics = &metrics;

        for saga_type in [
            SagaType::Provisioning,
            SagaType::Execution,
            SagaType::Recovery,
            SagaType::Cancellation,
            SagaType::Timeout,
            SagaType::Cleanup,
        ] {
            trait_metrics.record_saga_started(saga_type).await;
        }
    }
}
