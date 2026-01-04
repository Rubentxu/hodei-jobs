//! Metrics Module - Prometheus metrics for observability
//!
//! Provides comprehensive Prometheus metrics for:
//! - gRPC request latency and throughput
//! - Job lifecycle metrics
//! - Worker state metrics
//! - Infrastructure provider metrics
//!
//! EPIC-43: Sprint 5 - Observabilidad

use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Custom Histogram buckets for request latency
const LATENCY_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// gRPC Request metrics
#[derive(Debug)]
pub struct GrpcMetrics {
    /// Total requests by method
    pub requests_total: IntCounterVec,

    /// Request latency histogram
    pub request_latency: Histogram,

    /// Active requests (in-flight)
    pub active_requests: IntGauge,

    /// Request errors by type
    pub request_errors: IntCounterVec,

    /// Registry for metrics
    registry: Registry,
}

impl GrpcMetrics {
    /// Create new gRPC metrics
    pub fn new(service_name: &str) -> Result<Self, prometheus::Error> {
        let requests_total = IntCounterVec::new(
            Opts::new("grpc_requests_total", "Total gRPC requests")
                .const_labels(&[("service", service_name)]),
            &["method", "status"],
        );

        let request_latency = Histogram::with_opts(
            HistogramOpts::new("grpc_request_latency_seconds", "gRPC request latency")
                .const_labels(&[("service", service_name)])
                .buckets(LATENCY_BUCKETS.to_vec()),
        );

        let active_requests =
            IntGauge::new("grpc_active_requests", "Number of active gRPC requests")?;

        let request_errors = IntCounterVec::new(
            Opts::new("grpc_request_errors", "Total gRPC request errors")
                .const_labels(&[("service", service_name)]),
            &["method", "error_type"],
        );

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

    /// Record a request
    pub fn record_request(&self, method: &str, status: &str, latency: f64) {
        self.requests_total
            .with_label_values(&[method, status])
            .inc();
        self.request_latency.observe(latency);
    }

    /// Increment active requests
    pub fn inc_active(&self) {
        self.active_requests.inc();
    }

    /// Decrement active requests
    pub fn dec_active(&self) {
        self.active_requests.dec();
    }

    /// Record an error
    pub fn record_error(&self, method: &str, error_type: &str) {
        self.request_errors
            .with_label_values(&[method, error_type])
            .inc();
    }

    /// Get the registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Job lifecycle metrics
#[derive(Debug)]
pub struct JobMetrics {
    /// Jobs created
    pub jobs_created: IntCounter,

    /// Jobs queued
    pub jobs_queued: IntCounter,

    /// Jobs running
    pub jobs_running: IntGauge,

    /// Jobs completed
    pub jobs_completed: IntCounter,

    /// Jobs failed
    pub jobs_failed: IntCounter,

    /// Jobs cancelled
    pub jobs_cancelled: IntCounter,

    /// Job execution time histogram
    pub job_execution_time: Histogram,

    /// Current queue depth
    pub queue_depth: IntGauge,

    /// Registry
    registry: Registry,
}

impl JobMetrics {
    /// Create new job metrics
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

    /// Increment created counter
    pub fn inc_created(&self) {
        self.jobs_created.inc();
    }

    /// Increment queued counter
    pub fn inc_queued(&self) {
        self.jobs_queued.inc();
    }

    /// Increment running gauge
    pub fn inc_running(&self) {
        self.jobs_running.inc();
    }

    /// Decrement running gauge
    pub fn dec_running(&self) {
        self.jobs_running.dec();
    }

    /// Increment completed counter
    pub fn inc_completed(&self) {
        self.jobs_completed.inc();
    }

    /// Increment failed counter
    pub fn inc_failed(&self) {
        self.jobs_failed.inc();
    }

    /// Increment cancelled counter
    pub fn inc_cancelled(&self) {
        self.jobs_cancelled.inc();
    }

    /// Record execution time
    pub fn record_execution_time(&self, seconds: f64) {
        self.job_execution_time.observe(seconds);
    }

    /// Set queue depth
    pub fn set_queue_depth(&self, depth: i64) {
        self.queue_depth.set(depth);
    }

    /// Get the registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Worker state metrics
#[derive(Debug)]
pub struct WorkerMetrics {
    /// Workers by state
    pub workers_by_state: IntGaugeVec,

    /// Workers registered
    pub workers_registered: IntCounter,

    /// Workers terminated
    pub workers_terminated: IntCounter,

    /// Worker heartbeat latency
    pub heartbeat_latency: Histogram,

    /// Registry
    registry: Registry,
}

impl WorkerMetrics {
    /// Create new worker metrics
    pub fn new() -> Result<Self, prometheus::Error> {
        let workers_by_state = IntGaugeVec::new(
            Opts::new("hodei_workers_by_state", "Workers by state"),
            &["state"],
        );

        let workers_registered =
            IntCounter::new("hodei_workers_registered_total", "Total workers registered")?;

        let workers_terminated =
            IntCounter::new("hodei_workers_terminated_total", "Total workers terminated")?;

        let heartbeat_latency = Histogram::with_opts(HistogramOpts::new(
            "hodei_worker_heartbeat_latency_seconds",
            "Worker heartbeat latency",
        ))?;

        let registry = Registry::new();
        registry.register(Box::new(workers_by_state.clone()))?;
        registry.register(Box::new(workers_registered.clone()))?;
        registry.register(Box::new(workers_terminated.clone()))?;
        registry.register(Box::new(heartbeat_latency.clone()))?;

        Ok(Self {
            workers_by_state,
            workers_registered,
            workers_terminated,
            heartbeat_latency,
            registry,
        })
    }

    /// Set worker count for a state
    pub fn set_by_state(&self, state: &str, count: i64) {
        self.workers_by_state.with_label_values(&[state]).set(count);
    }

    /// Increment registered counter
    pub fn inc_registered(&self) {
        self.workers_registered.inc();
    }

    /// Increment terminated counter
    pub fn inc_terminated(&self) {
        self.workers_terminated.inc();
    }

    /// Record heartbeat latency
    pub fn record_heartbeat_latency(&self, seconds: f64) {
        self.heartbeat_latency.observe(seconds);
    }

    /// Get the registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Combined observability metrics
#[derive(Debug)]
pub struct ObservabilityMetrics {
    /// gRPC metrics
    pub grpc: GrpcMetrics,

    /// Job metrics
    pub jobs: JobMetrics,

    /// Worker metrics
    pub workers: WorkerMetrics,

    /// Registry for combining all metrics
    pub registry: Registry,
}

impl ObservabilityMetrics {
    /// Create new observability metrics
    pub fn new(service_name: &str) -> Result<Self, prometheus::Error> {
        let grpc = GrpcMetrics::new(service_name)?;
        let jobs = JobMetrics::new()?;
        let workers = WorkerMetrics::new()?;

        let mut registry = Registry::new();
        for metric in grpc.registry().metrics() {
            registry.register(metric.clone())?;
        }
        for metric in jobs.registry().metrics() {
            registry.register(metric.clone())?;
        }
        for metric in workers.registry().metrics() {
            registry.register(metric.clone())?;
        }

        Ok(Self {
            grpc,
            jobs,
            workers,
            registry,
        })
    }

    /// Get the combined registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Generate metrics output in Prometheus format
    pub fn gather(&self) -> Result<Vec<u8>, prometheus::Error> {
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(buffer)
    }
}

/// Global metrics instance (for use across the application)
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    /// Observability metrics
    pub metrics: Arc<RwLock<ObservabilityMetrics>>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    /// Create new metrics registry
    pub fn new() -> Self {
        let metrics = Arc::new(RwLock::new(
            ObservabilityMetrics::new("hodei-jobs").expect("Failed to create metrics"),
        ));

        Self { metrics }
    }

    /// Get gRPC metrics
    pub fn grpc(
        &self,
    ) -> std::sync::Guard<'_, tokio::sync::RwLockReadGuard<'_, ObservabilityMetrics>> {
        self.metrics.read().unwrap()
    }

    /// Get job metrics
    pub fn jobs(
        &self,
    ) -> std::sync::Guard<'_, tokio::sync::RwLockReadGuard<'_, ObservabilityMetrics>> {
        self.metrics.read().unwrap()
    }

    /// Get worker metrics
    pub fn workers(
        &self,
    ) -> std::sync::Guard<'_, tokio::sync::RwLockReadGuard<'_, ObservabilityMetrics>> {
        self.metrics.read().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_metrics_creation() {
        let metrics = GrpcMetrics::new("test-service").unwrap();
        assert!(metrics.registry().metrics().len() >= 4);
    }

    #[tokio::test]
    async fn test_job_metrics_creation() {
        let metrics = JobMetrics::new().unwrap();
        assert!(metrics.registry().metrics().len() >= 8);
    }

    #[tokio::test]
    async fn test_worker_metrics_creation() {
        let metrics = WorkerMetrics::new().unwrap();
        assert!(metrics.registry().metrics().len() >= 4);
    }

    #[tokio::test]
    async fn test_observability_metrics_creation() {
        let metrics = ObservabilityMetrics::new("test-service").unwrap();
        assert!(metrics.registry().metrics().len() >= 16);
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let grpc = GrpcMetrics::new("test").unwrap();

        grpc.record_request("QueueJob", "OK", 0.1);
        grpc.record_request("QueueJob", "OK", 0.2);
        grpc.record_request("GetJobStatus", "OK", 0.05);
        grpc.record_error("QueueJob", "timeout");

        grpc.inc_active();
        grpc.dec_active();
    }

    #[tokio::test]
    async fn test_metrics_gather() {
        let metrics = ObservabilityMetrics::new("test").unwrap();
        let output = metrics.gather().unwrap();
        let text = String::from_utf8(output).unwrap();

        // Verify some expected metrics are present
        assert!(text.contains("grpc_requests_total"));
        assert!(text.contains("hodei_jobs_created_total"));
        assert!(text.contains("hodei_workers_by_state"));
    }
}
