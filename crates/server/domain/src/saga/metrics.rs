//! Saga Metrics Interface - EPIC-46
//!
//! This module defines saga metrics for observability as specified in EPIC-46 Section 9.1.
//! Implementations are provided by the infrastructure layer using Prometheus.

use super::SagaType;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Trait for saga metrics collection (EPIC-46 Section 9.1)
///
/// Implement this trait in infrastructure layer to collect saga metrics.
/// The infrastructure crate provides a Prometheus-based implementation.
#[async_trait::async_trait]
pub trait SagaMetrics: Send + Sync {
    /// Record saga execution started
    async fn record_saga_started(&self, saga_type: SagaType);

    /// Record saga execution completed successfully
    async fn record_saga_completed(&self, saga_type: SagaType, duration: Duration);

    /// Record saga execution that required compensation
    async fn record_saga_compensated(&self, saga_type: SagaType, duration: Duration);

    /// Record saga execution failed
    async fn record_saga_failed(&self, saga_type: SagaType, error_type: &str);

    /// Record step execution latency
    async fn record_step_duration(&self, saga_type: SagaType, step_name: &str, duration: Duration);

    /// Increment active saga count
    async fn increment_active(&self, saga_type: SagaType);

    /// Decrement active saga count
    async fn decrement_active(&self, saga_type: SagaType);

    /// Record version conflict (optimistic locking)
    async fn record_version_conflict(&self, saga_type: SagaType);

    /// Record retry due to version conflict
    async fn record_retry(&self, saga_type: SagaType, reason: &str);
}

/// Metrics for saga concurrency monitoring (EPIC-46 Section 13.4)
#[derive(Debug, Default)]
pub struct SagaConcurrencyMetrics {
    /// Total number of version conflicts detected
    version_conflicts: AtomicU64,
    /// Total number of retries due to conflicts
    retry_count: AtomicU64,
    /// Current number of concurrent saga executions
    concurrent_executions: AtomicU64,
    /// Total number of sagas currently in compensating state
    compensating_count: AtomicU64,
}

impl Clone for SagaConcurrencyMetrics {
    fn clone(&self) -> Self {
        Self {
            version_conflicts: AtomicU64::new(self.version_conflicts.load(Ordering::SeqCst)),
            retry_count: AtomicU64::new(self.retry_count.load(Ordering::SeqCst)),
            concurrent_executions: AtomicU64::new(
                self.concurrent_executions.load(Ordering::SeqCst),
            ),
            compensating_count: AtomicU64::new(self.compensating_count.load(Ordering::SeqCst)),
        }
    }
}

impl SagaConcurrencyMetrics {
    /// Creates new concurrency metrics
    #[inline]
    pub fn new() -> Self {
        Self {
            version_conflicts: AtomicU64::new(0),
            retry_count: AtomicU64::new(0),
            concurrent_executions: AtomicU64::new(0),
            compensating_count: AtomicU64::new(0),
        }
    }

    /// Increment version conflict counter
    #[inline]
    pub fn record_version_conflict(&self) {
        self.version_conflicts.fetch_add(1, Ordering::SeqCst);
    }

    /// Get current version conflict count
    #[inline]
    pub fn version_conflict_count(&self) -> u64 {
        self.version_conflicts.load(Ordering::SeqCst)
    }

    /// Increment retry counter
    #[inline]
    pub fn record_retry(&self) {
        self.retry_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Get current retry count
    #[inline]
    pub fn retry_count(&self) -> u64 {
        self.retry_count.load(Ordering::SeqCst)
    }

    /// Increment concurrent execution count
    #[inline]
    pub fn increment_concurrent(&self) {
        self.concurrent_executions.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement concurrent execution count
    #[inline]
    pub fn decrement_concurrent(&self) {
        self.concurrent_executions.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get current concurrent execution count
    #[inline]
    pub fn concurrent_count(&self) -> u64 {
        self.concurrent_executions.load(Ordering::SeqCst)
    }

    /// Increment compensating count
    #[inline]
    pub fn increment_compensating(&self) {
        self.compensating_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement compensating count
    #[inline]
    pub fn decrement_compensating(&self) {
        self.compensating_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get current compensating count
    #[inline]
    pub fn compensating_count(&self) -> u64 {
        self.compensating_count.load(Ordering::SeqCst)
    }

    /// Reset all metrics
    #[inline]
    pub fn reset(&self) {
        self.version_conflicts.store(0, Ordering::SeqCst);
        self.retry_count.store(0, Ordering::SeqCst);
        self.concurrent_executions.store(0, Ordering::SeqCst);
        self.compensating_count.store(0, Ordering::SeqCst);
    }
}

/// No-op metrics implementation for testing (EPIC-46 Section 9.1)
#[derive(Debug, Clone, Default)]
pub struct NoOpSagaMetrics;

#[async_trait::async_trait]
impl SagaMetrics for NoOpSagaMetrics {
    async fn record_saga_started(&self, _saga_type: SagaType) {}

    async fn record_saga_completed(&self, _saga_type: SagaType, _duration: Duration) {}

    async fn record_saga_compensated(&self, _saga_type: SagaType, _duration: Duration) {}

    async fn record_saga_failed(&self, _saga_type: SagaType, _error_type: &str) {}

    async fn record_step_duration(
        &self,
        _saga_type: SagaType,
        _step_name: &str,
        _duration: Duration,
    ) {
    }

    async fn increment_active(&self, _saga_type: SagaType) {}

    async fn decrement_active(&self, _saga_type: SagaType) {}

    async fn record_version_conflict(&self, _saga_type: SagaType) {}

    async fn record_retry(&self, _saga_type: SagaType, _reason: &str) {}
}

/// In-memory metrics implementation for testing and development
#[derive(Debug, Default)]
pub struct InMemorySagaMetrics {
    started_count: AtomicU64,
    completed_count: AtomicU64,
    compensated_count: AtomicU64,
    failed_count: AtomicU64,
    active_count: AtomicU64,
}

impl Clone for InMemorySagaMetrics {
    fn clone(&self) -> Self {
        Self {
            started_count: AtomicU64::new(self.started_count.load(Ordering::SeqCst)),
            completed_count: AtomicU64::new(self.completed_count.load(Ordering::SeqCst)),
            compensated_count: AtomicU64::new(self.compensated_count.load(Ordering::SeqCst)),
            failed_count: AtomicU64::new(self.failed_count.load(Ordering::SeqCst)),
            active_count: AtomicU64::new(self.active_count.load(Ordering::SeqCst)),
        }
    }
}

impl InMemorySagaMetrics {
    /// Creates new in-memory metrics
    #[inline]
    pub fn new() -> Self {
        Self {
            started_count: AtomicU64::new(0),
            completed_count: AtomicU64::new(0),
            compensated_count: AtomicU64::new(0),
            failed_count: AtomicU64::new(0),
            active_count: AtomicU64::new(0),
        }
    }

    /// Get started count
    #[inline]
    pub fn started_count(&self) -> u64 {
        self.started_count.load(Ordering::SeqCst)
    }

    /// Get completed count
    #[inline]
    pub fn completed_count(&self) -> u64 {
        self.completed_count.load(Ordering::SeqCst)
    }

    /// Get compensated count
    #[inline]
    pub fn compensated_count(&self) -> u64 {
        self.compensated_count.load(Ordering::SeqCst)
    }

    /// Get failed count
    #[inline]
    pub fn failed_count(&self) -> u64 {
        self.failed_count.load(Ordering::SeqCst)
    }

    /// Get active count
    #[inline]
    pub fn active_count(&self) -> u64 {
        self.active_count.load(Ordering::SeqCst)
    }

    /// Calculate success rate
    #[inline]
    pub fn success_rate(&self) -> f64 {
        let total = self.completed_count() + self.compensated_count() + self.failed_count();
        if total == 0 {
            1.0
        } else {
            self.completed_count() as f64 / total as f64
        }
    }

    /// Reset all metrics
    #[inline]
    pub fn reset(&self) {
        self.started_count.store(0, Ordering::SeqCst);
        self.completed_count.store(0, Ordering::SeqCst);
        self.compensated_count.store(0, Ordering::SeqCst);
        self.failed_count.store(0, Ordering::SeqCst);
        self.active_count.store(0, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl SagaMetrics for InMemorySagaMetrics {
    async fn record_saga_started(&self, _saga_type: SagaType) {
        self.started_count.fetch_add(1, Ordering::SeqCst);
        self.active_count.fetch_add(1, Ordering::SeqCst);
    }

    async fn record_saga_completed(&self, _saga_type: SagaType, _duration: Duration) {
        self.completed_count.fetch_add(1, Ordering::SeqCst);
        self.active_count.fetch_sub(1, Ordering::SeqCst);
    }

    async fn record_saga_compensated(&self, _saga_type: SagaType, _duration: Duration) {
        self.compensated_count.fetch_add(1, Ordering::SeqCst);
        self.active_count.fetch_sub(1, Ordering::SeqCst);
    }

    async fn record_saga_failed(&self, _saga_type: SagaType, _error_type: &str) {
        self.failed_count.fetch_add(1, Ordering::SeqCst);
        self.active_count.fetch_sub(1, Ordering::SeqCst);
    }

    async fn record_step_duration(
        &self,
        _saga_type: SagaType,
        _step_name: &str,
        _duration: Duration,
    ) {
        // No-op for in-memory metrics (would histogram in real impl)
    }

    async fn increment_active(&self, _saga_type: SagaType) {
        self.active_count.fetch_add(1, Ordering::SeqCst);
    }

    async fn decrement_active(&self, _saga_type: SagaType) {
        self.active_count.fetch_sub(1, Ordering::SeqCst);
    }

    async fn record_version_conflict(&self, _saga_type: SagaType) {
        // Could add separate counter for version conflicts
        self.failed_count.fetch_add(1, Ordering::SeqCst);
    }

    async fn record_retry(&self, _saga_type: SagaType, _reason: &str) {
        // Could add retry counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_metrics_creation() {
        let metrics = InMemorySagaMetrics::new();
        assert_eq!(metrics.started_count(), 0);
        assert_eq!(metrics.completed_count(), 0);
    }

    #[test]
    fn test_concurrency_metrics_creation() {
        let metrics = SagaConcurrencyMetrics::new();
        assert_eq!(metrics.version_conflict_count(), 0);
        assert_eq!(metrics.concurrent_count(), 0);
    }

    #[test]
    fn test_success_rate_empty() {
        let metrics = InMemorySagaMetrics::new();
        assert_eq!(metrics.success_rate(), 1.0);
    }
}
