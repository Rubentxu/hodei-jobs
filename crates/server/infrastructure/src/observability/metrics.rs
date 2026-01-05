//! Metrics Module - Prometheus metrics for observability
//!
//! Provides basic Prometheus metrics for:
//! - gRPC request latency and throughput
//! - Job lifecycle metrics
//! - Worker state metrics
//!
//! EPIC-43: Sprint 5 - Observabilidad

use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry,
};
use std::collections::HashMap;

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

        let mut registry = Registry::new();
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

        let mut registry = Registry::new();
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

        let mut registry = Registry::new();
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
}

impl ObservabilityMetrics {
    pub fn new(service_name: &str) -> Result<Self, prometheus::Error> {
        let grpc = GrpcMetrics::new(service_name)?;
        let jobs = JobMetrics::new()?;
        let workers = WorkerMetrics::new()?;

        Ok(Self {
            grpc,
            jobs,
            workers,
        })
    }

    pub fn gather(&self) -> Result<Vec<u8>, prometheus::Error> {
        // Gather metrics from all sub-registries
        let mut families = Vec::new();
        families.extend_from_slice(&self.grpc.registry.gather());
        families.extend_from_slice(&self.jobs.registry.gather());
        families.extend_from_slice(&self.workers.registry.gather());

        let encoder = prometheus::TextEncoder::new();
        let mut buffer = Vec::new();
        encoder.encode(&families, &mut buffer)?;
        Ok(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
