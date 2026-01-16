//! Metrics Module - Prometheus metrics for observability
//!
//! Provides basic Prometheus metrics for:
//! - gRPC request latency and throughput
//! - Job lifecycle metrics
//! - Worker state metrics
//! - Saga execution metrics
//!
//! EPIC-43: Sprint 5 - Observabilidad
//! EPIC-85: US-04 - Prometheus SagaMetrics Implementation

use hodei_server_domain::saga::{SagaMetrics as SagaMetricsTrait, SagaType};
use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry,
};
use std::collections::HashMap;
use std::time::Duration;

/// Custom Histogram buckets for request latency
const LATENCY_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// gRPC Request metrics
#[derive(Debug)]
pub struct GrpcMetrics {
    pub requests_total: IntCounterVec,
    pub request_latency: Histogram,
    pub active_requests: IntGauge,
    pub request_errors: IntCounterVec,
    registry: Registry,
}

impl GrpcMetrics {
    pub fn new(service_name: &str) -> Result<Self, prometheus::Error> {
        let labels: HashMap<String, String> = [("service".into(), service_name.into())]
            .into_iter()
            .collect();

        let requests_total = IntCounterVec::new(
            Opts::new("grpc_requests_total", "Total gRPC requests").const_labels(labels.clone()),
            &["method", "status"],
        )?;

        let request_latency = Histogram::with_opts(
            HistogramOpts::new("grpc_request_latency_seconds", "gRPC request latency")
                .const_labels(labels.clone())
                .buckets(LATENCY_BUCKETS.to_vec()),
        )?;

        let active_requests =
            IntGauge::new("grpc_active_requests", "Number of active gRPC requests")?;

        let request_errors = IntCounterVec::new(
            Opts::new("grpc_request_errors", "Total gRPC request errors").const_labels(labels),
            &["method", "error_type"],
        )?;

        let registry = Registry::new();
        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_latency.clone()))?;
        registry.register(Box::new(active_requests.clone()))?;
        registry.register(Box::new(request_errors.clone()))?;

        Ok(Self {
            requests_total,
            request_latency,
            active_requests,
            request_errors,
            registry,
        })
    }

    pub fn record_request(&self, method: &str, status: &str, latency: f64) {
        self.requests_total
            .with_label_values(&[method, status])
            .inc();
        self.request_latency.observe(latency);
    }

    pub fn inc_active(&self) {
        self.active_requests.inc();
    }
    pub fn dec_active(&self) {
        self.active_requests.dec();
    }
    pub fn record_error(&self, method: &str, error_type: &str) {
        self.request_errors
            .with_label_values(&[method, error_type])
            .inc();
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Job lifecycle metrics
#[derive(Debug)]
pub struct JobMetrics {
    pub jobs_created: IntCounter,
    pub jobs_queued: IntCounter,
    pub jobs_running: IntGauge,
    pub jobs_completed: IntCounter,
    pub jobs_failed: IntCounter,
    pub jobs_cancelled: IntCounter,
    pub job_execution_time: Histogram,
    pub queue_depth: IntGauge,
    registry: Registry,
}

impl JobMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let jobs_created = IntCounter::new("hodei_jobs_created_total", "Total jobs created")?;
        let jobs_queued = IntCounter::new("hodei_jobs_queued_total", "Total jobs queued")?;
        let jobs_running = IntGauge::new("hodei_jobs_running", "Number of currently running jobs")?;
        let jobs_completed = IntCounter::new("hodei_jobs_completed_total", "Total jobs completed")?;
        let jobs_failed = IntCounter::new("hodei_jobs_failed_total", "Total jobs failed")?;
        let jobs_cancelled = IntCounter::new("hodei_jobs_cancelled_total", "Total jobs cancelled")?;
        let job_execution_time = Histogram::with_opts(
            HistogramOpts::new(
                "hodei_job_execution_time_seconds",
                "Job execution time histogram",
            )
            .buckets(LATENCY_BUCKETS.to_vec()),
        )?;
        let queue_depth = IntGauge::new("hodei_job_queue_depth", "Current job queue depth")?;

        let registry = Registry::new();
        registry.register(Box::new(jobs_created.clone()))?;
        registry.register(Box::new(jobs_queued.clone()))?;
        registry.register(Box::new(jobs_running.clone()))?;
        registry.register(Box::new(jobs_completed.clone()))?;
        registry.register(Box::new(jobs_failed.clone()))?;
        registry.register(Box::new(jobs_cancelled.clone()))?;
        registry.register(Box::new(job_execution_time.clone()))?;
        registry.register(Box::new(queue_depth.clone()))?;

        Ok(Self {
            jobs_created,
            jobs_queued,
            jobs_running,
            jobs_completed,
            jobs_failed,
            jobs_cancelled,
            job_execution_time,
            queue_depth,
            registry,
        })
    }

    pub fn inc_created(&self) {
        self.jobs_created.inc();
    }
    pub fn inc_queued(&self) {
        self.jobs_queued.inc();
    }
    pub fn inc_running(&self) {
        self.jobs_running.inc();
    }
    pub fn dec_running(&self) {
        self.jobs_running.dec();
    }
    pub fn inc_completed(&self) {
        self.jobs_completed.inc();
    }
    pub fn inc_failed(&self) {
        self.jobs_failed.inc();
    }
    pub fn inc_cancelled(&self) {
        self.jobs_cancelled.inc();
    }
    pub fn record_execution_time(&self, seconds: f64) {
        self.job_execution_time.observe(seconds);
    }
    pub fn set_queue_depth(&self, depth: i64) {
        self.queue_depth.set(depth);
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Worker state metrics
#[derive(Debug)]
pub struct WorkerMetrics {
    pub workers_by_state: IntGaugeVec,
    pub workers_registered: IntCounter,
    pub workers_active: IntGauge,
    pub workers_idle: IntGauge,
    pub workers_errors: IntCounter,
    registry: Registry,
}

impl WorkerMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let workers_by_state = IntGaugeVec::new(
            Opts::new("hodei_workers_by_state", "Workers by state"),
            &["state"],
        )?;
        let workers_registered =
            IntCounter::new("hodei_workers_registered_total", "Total workers registered")?;
        let workers_active = IntGauge::new("hodei_workers_active", "Number of active workers")?;
        let workers_idle = IntGauge::new("hodei_workers_idle", "Number of idle workers")?;
        let workers_errors = IntCounter::new("hodei_workers_errors_total", "Total worker errors")?;

        let registry = Registry::new();
        registry.register(Box::new(workers_by_state.clone()))?;
        registry.register(Box::new(workers_registered.clone()))?;
        registry.register(Box::new(workers_active.clone()))?;
        registry.register(Box::new(workers_idle.clone()))?;
        registry.register(Box::new(workers_errors.clone()))?;

        Ok(Self {
            workers_by_state,
            workers_registered,
            workers_active,
            workers_idle,
            workers_errors,
            registry,
        })
    }

    pub fn inc_registered(&self) {
        self.workers_registered.inc();
    }
    pub fn set_by_state(&self, state: &str, count: i64) {
        self.workers_by_state.with_label_values(&[state]).set(count);
    }
    pub fn inc_active(&self) {
        self.workers_active.inc();
    }
    pub fn dec_active(&self) {
        self.workers_active.dec();
    }
    pub fn inc_idle(&self) {
        self.workers_idle.inc();
    }
    pub fn dec_idle(&self) {
        self.workers_idle.dec();
    }
    pub fn inc_errors(&self) {
        self.workers_errors.inc();
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Combined observability metrics
#[derive(Debug)]
pub struct ObservabilityMetrics {
    pub grpc: GrpcMetrics,
    pub jobs: JobMetrics,
    pub workers: WorkerMetrics,
    pub saga: PrometheusSagaMetrics,
}

impl ObservabilityMetrics {
    pub fn new(service_name: &str) -> Result<Self, prometheus::Error> {
        let grpc = GrpcMetrics::new(service_name)?;
        let jobs = JobMetrics::new()?;
        let workers = WorkerMetrics::new()?;
        let saga = PrometheusSagaMetrics::new()?;

        Ok(Self {
            grpc,
            jobs,
            workers,
            saga,
        })
    }

    pub fn gather(&self) -> Result<Vec<u8>, prometheus::Error> {
        // Gather metrics from all sub-registries
        let mut families = Vec::new();
        families.extend_from_slice(&self.grpc.registry.gather());
        families.extend_from_slice(&self.jobs.registry.gather());
        families.extend_from_slice(&self.workers.registry.gather());
        families.extend_from_slice(&self.saga.registry.gather());

        let encoder = prometheus::TextEncoder::new();
        let mut buffer = Vec::new();
        encoder.encode(&families, &mut buffer)?;
        Ok(buffer)
    }
}

// ============================================================================
// Prometheus Saga Metrics (EPIC-85 US-04)
// ============================================================================

/// Prometheus-based implementation of SagaMetrics.
///
/// This implementation records all saga metrics to Prometheus counters
/// and histograms for production observability.
#[derive(Debug)]
pub struct PrometheusSagaMetrics {
    /// Total sagas started by type
    saga_started: IntCounterVec,
    /// Total sagas completed by type
    saga_completed: IntCounterVec,
    /// Total sagas that required compensation by type
    saga_compensated: IntCounterVec,
    /// Total sagas failed by type and error type
    saga_failed: IntCounterVec,
    /// Histogram of saga duration by type
    saga_duration: Histogram,
    /// Counter of step duration in milliseconds by type and step name
    step_duration: IntCounterVec,
    /// Current number of active sagas by type
    saga_active: IntGaugeVec,
    /// Current number of sagas in compensating state by type
    saga_compensating: IntGaugeVec,
    /// Total version conflicts by type
    version_conflicts: IntCounterVec,
    /// Total retries by type and reason
    saga_retries: IntCounterVec,
    /// Registry for prometheus metrics
    registry: Registry,
}

impl PrometheusSagaMetrics {
    /// Creates a new PrometheusSagaMetrics instance.
    pub fn new() -> Result<Self, prometheus::Error> {
        let saga_started = IntCounterVec::new(
            Opts::new(
                "hodei_saga_started_total",
                "Total number of sagas started by type",
            ),
            &["saga_type"],
        )?;

        let saga_completed = IntCounterVec::new(
            Opts::new(
                "hodei_saga_completed_total",
                "Total number of sagas completed successfully by type",
            ),
            &["saga_type"],
        )?;

        let saga_compensated = IntCounterVec::new(
            Opts::new(
                "hodei_saga_compensated_total",
                "Total number of sagas that required compensation by type",
            ),
            &["saga_type"],
        )?;

        let saga_failed = IntCounterVec::new(
            Opts::new(
                "hodei_saga_failed_total",
                "Total number of sagas that failed by type and error type",
            ),
            &["saga_type", "error_type"],
        )?;

        let saga_duration = Histogram::with_opts(
            HistogramOpts::new(
                "hodei_saga_duration_seconds",
                "Saga execution duration in seconds by type",
            )
            .buckets(LATENCY_BUCKETS.to_vec()),
        )?;

        let step_duration = IntCounterVec::new(
            Opts::new(
                "hodei_saga_step_duration_ms",
                "Step execution duration in milliseconds by saga type and step name",
            ),
            &["saga_type", "step_name"],
        )?;

        let saga_active = IntGaugeVec::new(
            Opts::new(
                "hodei_saga_active",
                "Current number of active sagas by type",
            ),
            &["saga_type"],
        )?;

        let saga_compensating = IntGaugeVec::new(
            Opts::new(
                "hodei_saga_compensating",
                "Current number of sagas in compensating state by type",
            ),
            &["saga_type"],
        )?;

        let version_conflicts = IntCounterVec::new(
            Opts::new(
                "hodei_saga_version_conflicts_total",
                "Total number of version conflicts by saga type",
            ),
            &["saga_type"],
        )?;

        let saga_retries = IntCounterVec::new(
            Opts::new(
                "hodei_saga_retries_total",
                "Total number of retries by saga type and reason",
            ),
            &["saga_type", "reason"],
        )?;

        let registry = Registry::new();
        registry.register(Box::new(saga_started.clone()))?;
        registry.register(Box::new(saga_completed.clone()))?;
        registry.register(Box::new(saga_compensated.clone()))?;
        registry.register(Box::new(saga_failed.clone()))?;
        registry.register(Box::new(saga_duration.clone()))?;
        registry.register(Box::new(step_duration.clone()))?;
        registry.register(Box::new(saga_active.clone()))?;
        registry.register(Box::new(saga_compensating.clone()))?;
        registry.register(Box::new(version_conflicts.clone()))?;
        registry.register(Box::new(saga_retries.clone()))?;

        Ok(Self {
            saga_started,
            saga_completed,
            saga_compensated,
            saga_failed,
            saga_duration,
            step_duration,
            saga_active,
            saga_compensating,
            version_conflicts,
            saga_retries,
            registry,
        })
    }

    /// Creates a new PrometheusSagaMetrics with the default registry.
    #[inline]
    pub fn with_default_registry() -> Result<Self, prometheus::Error> {
        Self::new()
    }

    /// Gets the Prometheus registry.
    #[inline]
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Helper to get saga_type label as string.
    #[inline]
    fn saga_type_label(saga_type: SagaType) -> &'static str {
        match saga_type {
            SagaType::Provisioning => "provisioning",
            SagaType::Execution => "execution",
            SagaType::Recovery => "recovery",
            SagaType::Cancellation => "cancellation",
            SagaType::Timeout => "timeout",
            SagaType::Cleanup => "cleanup",
        }
    }
}

impl Default for PrometheusSagaMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create PrometheusSagaMetrics")
    }
}

#[async_trait::async_trait]
impl SagaMetricsTrait for PrometheusSagaMetrics {
    async fn record_saga_started(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(saga_type);
        self.saga_started.with_label_values(&[label]).inc();
        self.saga_active.with_label_values(&[label]).inc();
    }

    async fn record_saga_completed(&self, saga_type: SagaType, duration: Duration) {
        let label = Self::saga_type_label(saga_type);
        self.saga_completed.with_label_values(&[label]).inc();
        self.saga_active.with_label_values(&[label]).dec();
        self.saga_duration.observe(duration.as_secs_f64());
    }

    async fn record_saga_compensated(&self, saga_type: SagaType, duration: Duration) {
        let label = Self::saga_type_label(saga_type);
        self.saga_compensated.with_label_values(&[label]).inc();
        self.saga_active.with_label_values(&[label]).dec();
        self.saga_compensating.with_label_values(&[label]).dec();
        self.saga_duration.observe(duration.as_secs_f64());
    }

    async fn record_saga_failed(&self, saga_type: SagaType, error_type: &str) {
        let label = Self::saga_type_label(saga_type);
        self.saga_failed
            .with_label_values(&[label, error_type])
            .inc();
        self.saga_active.with_label_values(&[label]).dec();
    }

    async fn record_step_duration(&self, saga_type: SagaType, step_name: &str, duration: Duration) {
        let saga_label = Self::saga_type_label(saga_type);
        self.step_duration
            .with_label_values(&[saga_label, step_name])
            .inc_by(duration.as_millis() as u64);
    }

    async fn increment_active(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(saga_type);
        self.saga_active.with_label_values(&[label]).inc();
    }

    async fn decrement_active(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(saga_type);
        self.saga_active.with_label_values(&[label]).dec();
    }

    async fn record_version_conflict(&self, saga_type: SagaType) {
        let label = Self::saga_type_label(saga_type);
        self.version_conflicts.with_label_values(&[label]).inc();
    }

    async fn record_retry(&self, saga_type: SagaType, reason: &str) {
        let label = Self::saga_type_label(saga_type);
        self.saga_retries.with_label_values(&[label, reason]).inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_grpc_metrics_creation() {
        let metrics = GrpcMetrics::new("test-service").unwrap();
        metrics.record_request("GetJob", "ok", 0.1);
        metrics.inc_active();
        metrics.dec_active();
        metrics.record_error("CreateJob", "invalid_arg");
    }

    #[tokio::test]
    async fn test_job_metrics_creation() {
        let metrics = JobMetrics::new().unwrap();
        metrics.inc_created();
        metrics.inc_queued();
        metrics.inc_running();
        metrics.dec_running();
        metrics.inc_completed();
        metrics.inc_failed();
        metrics.inc_cancelled();
        metrics.record_execution_time(1.5);
        metrics.set_queue_depth(10);
    }

    #[tokio::test]
    async fn test_worker_metrics_creation() {
        let metrics = WorkerMetrics::new().unwrap();
        metrics.inc_registered();
        metrics.set_by_state("running", 5);
        metrics.set_by_state("idle", 3);
        metrics.inc_active();
        metrics.dec_active();
        metrics.inc_idle();
        metrics.dec_idle();
        metrics.inc_errors();
    }

    #[tokio::test]
    async fn test_observability_metrics_creation() {
        let metrics = ObservabilityMetrics::new("test-service").unwrap();
        let output = metrics.gather().unwrap();
        assert!(!output.is_empty());
    }

    // EPIC-85 US-04: Prometheus SagaMetrics Tests
    #[tokio::test]
    async fn test_prometheus_saga_metrics_creation() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        let output = metrics.registry.gather();
        assert!(!output.is_empty());
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_record_started() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics.record_saga_started(SagaType::Provisioning).await;
        metrics.record_saga_started(SagaType::Execution).await;
        metrics.record_saga_started(SagaType::Recovery).await;
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_record_completed() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics
            .record_saga_completed(SagaType::Execution, Duration::from_secs(30))
            .await;
        metrics
            .record_saga_completed(SagaType::Provisioning, Duration::from_secs(45))
            .await;
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_record_compensated() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics
            .record_saga_compensated(SagaType::Recovery, Duration::from_secs(60))
            .await;
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_record_failed() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics
            .record_saga_failed(SagaType::Execution, "timeout")
            .await;
        metrics
            .record_saga_failed(SagaType::Provisioning, "infrastructure_error")
            .await;
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_record_step_duration() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics
            .record_step_duration(
                SagaType::Execution,
                "ValidateJob",
                Duration::from_millis(100),
            )
            .await;
        metrics
            .record_step_duration(
                SagaType::Execution,
                "AssignWorker",
                Duration::from_millis(200),
            )
            .await;
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_increment_decrement_active() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics.increment_active(SagaType::Cleanup).await;
        metrics.increment_active(SagaType::Cleanup).await;
        metrics.decrement_active(SagaType::Cleanup).await;
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_record_version_conflict() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics.record_version_conflict(SagaType::Execution).await;
        metrics.record_version_conflict(SagaType::Recovery).await;
    }

    #[tokio::test]
    async fn test_prometheus_saga_metrics_record_retry() {
        let metrics = PrometheusSagaMetrics::new().unwrap();
        metrics
            .record_retry(SagaType::Execution, "optimistic_lock")
            .await;
        metrics
            .record_retry(SagaType::Execution, "resource_busy")
            .await;
    }

    #[tokio::test]
    async fn test_observability_metrics_includes_saga() {
        // Test that PrometheusSagaMetrics has correct registry content
        let saga_metrics = PrometheusSagaMetrics::new().unwrap();
        // Record some metrics
        saga_metrics.record_saga_started(SagaType::Execution).await;
        saga_metrics
            .record_saga_completed(SagaType::Execution, Duration::from_secs(30))
            .await;
        // Verify metrics were created successfully by checking registry
        let families = saga_metrics.registry.gather();
        assert!(!families.is_empty());
        // Check that we can encode the metrics (they exist in the registry)
        let encoder = prometheus::TextEncoder::new();
        let mut buffer = Vec::new();
        let result = encoder.encode(&families, &mut buffer);
        assert!(result.is_ok());
        let output_str = String::from_utf8_lossy(&buffer);
        assert!(output_str.contains("hodei_saga_started_total"));
    }
}
