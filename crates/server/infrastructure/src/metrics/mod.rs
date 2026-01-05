//! Prometheus Metrics Infrastructure
//!
//! This module provides comprehensive metrics collection for the Hodei Job Platform,
//! including business metrics, system metrics, performance metrics, and security metrics.

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
};
use std::sync::Arc;

/// Global metrics registry
pub struct MetricsRegistry {
    registry: Registry,
    business_metrics: BusinessMetrics,
    system_metrics: SystemMetrics,
    performance_metrics: PerformanceMetrics,
    security_metrics: SecurityMetrics,
    /// EPIC-45 Gap 3: Saga metrics for observability
    saga_metrics: SagaMetrics,
}

impl MetricsRegistry {
    /// Create a new metrics registry with all metrics initialized
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let business_metrics = BusinessMetrics::register(&registry)?;
        let system_metrics = SystemMetrics::register(&registry)?;
        let performance_metrics = PerformanceMetrics::register(&registry)?;
        let security_metrics = SecurityMetrics::register(&registry)?;
        // EPIC-45 Gap 3: Register saga metrics
        let saga_metrics = SagaMetrics::register(&registry)?;

        Ok(Self {
            registry,
            business_metrics,
            system_metrics,
            performance_metrics,
            security_metrics,
            saga_metrics,
        })
    }

    /// Get the Prometheus registry for serving metrics
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get business metrics collector
    pub fn business(&self) -> &BusinessMetrics {
        &self.business_metrics
    }

    /// Get system metrics collector
    pub fn system(&self) -> &SystemMetrics {
        &self.system_metrics
    }

    /// Get performance metrics collector
    pub fn performance(&self) -> &PerformanceMetrics {
        &self.performance_metrics
    }

    /// Get security metrics collector
    pub fn security(&self) -> &SecurityMetrics {
        &self.security_metrics
    }

    /// EPIC-45 Gap 3: Get saga metrics collector
    pub fn saga(&self) -> &SagaMetrics {
        &self.saga_metrics
    }
}

/// Business Metrics - Track job and worker lifecycle events
pub struct BusinessMetrics {
    /// Total number of jobs created
    pub jobs_created: IntCounterVec,
    /// Total number of jobs completed
    pub jobs_completed: IntCounterVec,
    /// Total number of jobs failed
    pub jobs_failed: IntCounterVec,
    /// Job duration histogram (in seconds)
    pub job_duration: HistogramVec,
    /// Current number of active workers
    pub worker_count: IntGaugeVec,
    /// Current queue depth
    pub queue_depth: IntGauge,
    /// Provider health status
    pub provider_health: IntGaugeVec,
}

impl BusinessMetrics {
    fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let jobs_created = IntCounterVec::new(
            Opts::new("hodei_jobs_created_total", "Total number of jobs created")
                .variable_labels(&["job_type", "provider", "region"])
                .const_label("component", "business"),
            &["job_type", "provider", "region"],
        )?;
        registry.register(Box::new(jobs_created.clone()))?;

        let jobs_completed = IntCounterVec::new(
            Opts::new(
                "hodei_jobs_completed_total",
                "Total number of jobs completed",
            )
            .variable_labels(&["job_type", "provider", "region", "status"])
            .const_label("component", "business"),
            &["job_type", "provider", "region", "status"],
        )?;
        registry.register(Box::new(jobs_completed.clone()))?;

        let jobs_failed = IntCounterVec::new(
            Opts::new("hodei_jobs_failed_total", "Total number of jobs failed")
                .variable_labels(&["job_type", "provider", "region", "error_type"])
                .const_label("component", "business"),
            &["job_type", "provider", "region", "error_type"],
        )?;
        registry.register(Box::new(jobs_failed.clone()))?;

        let job_duration = HistogramVec::new(
            HistogramOpts::new(
                "hodei_job_duration_seconds",
                "Job execution duration in seconds",
            )
            .variable_labels(&["job_type", "provider"])
            .const_label("component", "business")
            .buckets(vec![
                0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0,
            ]),
            &["job_type", "provider"],
        )?;
        registry.register(Box::new(job_duration.clone()))?;

        let worker_count = IntGaugeVec::new(
            Opts::new("hodei_workers_total", "Total number of workers")
                .variable_labels(&["provider", "state"])
                .const_label("component", "business"),
            &["provider", "state"],
        )?;
        registry.register(Box::new(worker_count.clone()))?;

        let queue_depth = IntGauge::with_opts(
            Opts::new("hodei_queue_depth", "Current depth of job queue")
                .const_label("component", "business"),
        )?;
        registry.register(Box::new(queue_depth.clone()))?;

        let provider_health = IntGaugeVec::new(
            Opts::new(
                "hodei_provider_health",
                "Provider health status (1=healthy, 0=unhealthy)",
            )
            .variable_labels(&["provider", "region"])
            .const_label("component", "business"),
            &["provider", "region"],
        )?;
        registry.register(Box::new(provider_health.clone()))?;

        Ok(Self {
            jobs_created,
            jobs_completed,
            jobs_failed,
            job_duration,
            worker_count,
            queue_depth,
            provider_health,
        })
    }
}

/// System Metrics - Track system-level metrics
pub struct SystemMetrics {
    /// Total number of events processed
    pub events_processed: IntCounterVec,
    /// Event processing latency
    pub event_processing_latency: HistogramVec,
    /// Number of active database connections
    pub db_connections_active: IntGauge,
    /// Database query duration
    pub db_query_duration: HistogramVec,
    /// Provider health check response time
    pub provider_health_check_duration: HistogramVec,
    /// Provider health check success count
    pub provider_health_check_success: IntCounterVec,
    /// Provider health check error count
    pub provider_health_check_errors: IntCounterVec,
    /// Provider health check total count
    pub provider_health_check_total: IntCounterVec,
}

impl SystemMetrics {
    fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let events_processed = IntCounterVec::new(
            Opts::new(
                "hodei_events_processed_total",
                "Total number of events processed",
            )
            .variable_labels(&["event_type"])
            .const_label("component", "system"),
            &["event_type"],
        )?;
        registry.register(Box::new(events_processed.clone()))?;

        let event_processing_latency = HistogramVec::new(
            HistogramOpts::new(
                "hodei_event_processing_latency_seconds",
                "Event processing latency in seconds",
            )
            .variable_labels(&["event_type"])
            .const_label("component", "system")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["event_type"],
        )?;
        registry.register(Box::new(event_processing_latency.clone()))?;

        let db_connections_active = IntGauge::with_opts(
            Opts::new(
                "hodei_db_connections_active",
                "Number of active database connections",
            )
            .const_label("component", "system"),
        )?;
        registry.register(Box::new(db_connections_active.clone()))?;

        let db_query_duration = HistogramVec::new(
            HistogramOpts::new(
                "hodei_db_query_duration_seconds",
                "Database query duration in seconds",
            )
            .variable_labels(&["query_type"])
            .const_label("component", "system")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["query_type"],
        )?;
        registry.register(Box::new(db_query_duration.clone()))?;

        let provider_health_check_duration = HistogramVec::new(
            HistogramOpts::new(
                "hodei_provider_health_check_duration_seconds",
                "Provider health check response time in seconds",
            )
            .variable_labels(&["provider", "provider_type"])
            .const_label("component", "system")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["provider", "provider_type"],
        )?;
        registry.register(Box::new(provider_health_check_duration.clone()))?;

        let provider_health_check_success = IntCounterVec::new(
            Opts::new(
                "hodei_provider_health_check_success_total",
                "Total number of successful provider health checks",
            )
            .variable_labels(&["provider", "provider_type"])
            .const_label("component", "system"),
            &["provider", "provider_type"],
        )?;
        registry.register(Box::new(provider_health_check_success.clone()))?;

        let provider_health_check_errors = IntCounterVec::new(
            Opts::new(
                "hodei_provider_health_check_errors_total",
                "Total number of failed provider health checks",
            )
            .variable_labels(&["provider", "provider_type", "error_type"])
            .const_label("component", "system"),
            &["provider", "provider_type", "error_type"],
        )?;
        registry.register(Box::new(provider_health_check_errors.clone()))?;

        let provider_health_check_total = IntCounterVec::new(
            Opts::new(
                "hodei_provider_health_check_total",
                "Total number of provider health checks performed",
            )
            .variable_labels(&["provider", "provider_type"])
            .const_label("component", "system"),
            &["provider", "provider_type"],
        )?;
        registry.register(Box::new(provider_health_check_total.clone()))?;

        Ok(Self {
            events_processed,
            event_processing_latency,
            db_connections_active,
            db_query_duration,
            provider_health_check_duration,
            provider_health_check_success,
            provider_health_check_errors,
            provider_health_check_total,
        })
    }
}

/// Performance Metrics - Track performance-related metrics
pub struct PerformanceMetrics {
    /// gRPC requests per second
    pub grpc_rps: GaugeVec,
    /// gRPC request latency
    pub grpc_request_latency: HistogramVec,
    /// Log batch processing rate
    pub log_batch_rate: Gauge,
    /// Log entries per second
    pub log_entries_per_second: Gauge,
    /// Worker provisioning time
    pub worker_provisioning_time: HistogramVec,
}

impl PerformanceMetrics {
    fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let grpc_rps = GaugeVec::new(
            Opts::new("hodei_grpc_rps", "gRPC requests per second")
                .variable_labels(&["service", "method"])
                .const_label("component", "performance"),
            &["service", "method"],
        )?;
        registry.register(Box::new(grpc_rps.clone()))?;

        let grpc_request_latency = HistogramVec::new(
            HistogramOpts::new(
                "hodei_grpc_request_latency_seconds",
                "gRPC request latency in seconds",
            )
            .variable_labels(&["service", "method", "status"])
            .const_label("component", "performance")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["service", "method", "status"],
        )?;
        registry.register(Box::new(grpc_request_latency.clone()))?;

        let log_batch_rate = Gauge::with_opts(
            Opts::new(
                "hodei_log_batch_rate",
                "Log batch processing rate (batches/sec)",
            )
            .const_label("component", "performance"),
        )?;
        registry.register(Box::new(log_batch_rate.clone()))?;

        let log_entries_per_second = Gauge::with_opts(
            Opts::new(
                "hodei_log_entries_per_second",
                "Log entries processed per second",
            )
            .const_label("component", "performance"),
        )?;
        registry.register(Box::new(log_entries_per_second.clone()))?;

        let worker_provisioning_time = HistogramVec::new(
            HistogramOpts::new(
                "hodei_worker_provisioning_time_seconds",
                "Worker provisioning time in seconds",
            )
            .variable_labels(&["provider"])
            .const_label("component", "performance")
            .buckets(vec![0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0]),
            &["provider"],
        )?;
        registry.register(Box::new(worker_provisioning_time.clone()))?;

        Ok(Self {
            grpc_rps,
            grpc_request_latency,
            log_batch_rate,
            log_entries_per_second,
            worker_provisioning_time,
        })
    }
}

/// Security Metrics - Track security-related metrics
pub struct SecurityMetrics {
    /// Number of secret accesses
    pub secret_access_count: IntCounterVec,
    /// Certificate rotation status
    pub cert_rotation_status: IntGaugeVec,
    /// Certificate expiration time
    pub cert_expiration_hours: GaugeVec,
    /// Failed authentication attempts
    pub auth_failures: IntCounterVec,
    /// Successful authentications
    pub auth_success: IntCounterVec,
}

impl SecurityMetrics {
    fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let secret_access_count = IntCounterVec::new(
            Opts::new(
                "hodei_secret_access_count",
                "Number of times secrets have been accessed",
            )
            .variable_labels(&["job_id", "secret_type"])
            .const_label("component", "security"),
            &["job_id", "secret_type"],
        )?;
        registry.register(Box::new(secret_access_count.clone()))?;

        let cert_rotation_status = IntGaugeVec::new(
            Opts::new(
                "hodei_cert_rotation_status",
                "Certificate rotation status (1=success, 0=failure)",
            )
            .variable_labels(&["cert_type"])
            .const_label("component", "security"),
            &["cert_type"],
        )?;
        registry.register(Box::new(cert_rotation_status.clone()))?;

        let cert_expiration_hours = GaugeVec::new(
            Opts::new(
                "hodei_cert_expiration_hours",
                "Time until certificate expiration in hours",
            )
            .variable_labels(&["cert_type"])
            .const_label("component", "security"),
            &["cert_type"],
        )?;
        registry.register(Box::new(cert_expiration_hours.clone()))?;

        let auth_failures = IntCounterVec::new(
            Opts::new(
                "hodei_auth_failures_total",
                "Total number of authentication failures",
            )
            .variable_labels(&["auth_type", "reason"])
            .const_label("component", "security"),
            &["auth_type", "reason"],
        )?;
        registry.register(Box::new(auth_failures.clone()))?;

        let auth_success = IntCounterVec::new(
            Opts::new(
                "hodei_auth_success_total",
                "Total number of successful authentications",
            )
            .variable_labels(&["auth_type"])
            .const_label("component", "security"),
            &["auth_type"],
        )?;
        registry.register(Box::new(auth_success.clone()))?;

        Ok(Self {
            secret_access_count,
            cert_rotation_status,
            cert_expiration_hours,
            auth_failures,
            auth_success,
        })
    }
}

/// Global metrics registry instance
use once_cell::sync::Lazy;
pub static METRICS_REGISTRY: Lazy<Arc<MetricsRegistry>> =
    Lazy::new(|| Arc::new(MetricsRegistry::new().expect("Failed to initialize metrics registry")));

/// Prometheus-based saga metrics implementation
/// EPIC-45 Gap 3: Wraps Prometheus metrics with domain trait
#[derive(Clone)]
pub struct PrometheusSagaMetrics {
    /// Total number of saga executions started
    saga_started: Arc<IntCounterVec>,
    /// Total number of saga executions completed successfully
    saga_completed: Arc<IntCounterVec>,
    /// Total number of saga executions that required compensation
    saga_compensated: Arc<IntCounterVec>,
    /// Total number of saga executions that failed
    saga_failed: Arc<IntCounterVec>,
    /// Saga execution duration histogram
    saga_duration_seconds: Arc<HistogramVec>,
    /// Number of compensations executed
    saga_compensation_total: Arc<IntCounterVec>,
    /// Current number of active sagas
    saga_active: Arc<IntGaugeVec>,
    /// Saga step execution latency
    saga_step_duration_seconds: Arc<HistogramVec>,
    /// Prometheus registry for metrics
    registry: Registry,
}

impl PrometheusSagaMetrics {
    /// Create a new Prometheus saga metrics instance
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let saga_started = IntCounterVec::new(
            Opts::new(
                "hodei_saga_started_total",
                "Total number of saga executions started",
            )
            .variable_labels(&["saga_type"])
            .const_label("component", "saga"),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_started.clone()))?;

        let saga_completed = IntCounterVec::new(
            Opts::new(
                "hodei_saga_completed_total",
                "Total number of saga executions completed successfully",
            )
            .variable_labels(&["saga_type"])
            .const_label("component", "saga"),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_completed.clone()))?;

        let saga_compensated = IntCounterVec::new(
            Opts::new(
                "hodei_saga_compensated_total",
                "Total number of saga executions that required compensation",
            )
            .variable_labels(&["saga_type", "compensation_step"])
            .const_label("component", "saga"),
            &["saga_type", "compensation_step"],
        )?;
        registry.register(Box::new(saga_compensated.clone()))?;

        let saga_failed = IntCounterVec::new(
            Opts::new(
                "hodei_saga_failed_total",
                "Total number of saga executions that failed",
            )
            .variable_labels(&["saga_type", "error_type"])
            .const_label("component", "saga"),
            &["saga_type", "error_type"],
        )?;
        registry.register(Box::new(saga_failed.clone()))?;

        let saga_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "hodei_saga_duration_seconds",
                "Saga execution duration in seconds",
            )
            .variable_labels(&["saga_type"])
            .const_label("component", "saga")
            .buckets(vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0]),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_duration_seconds.clone()))?;

        let saga_compensation_total = IntCounterVec::new(
            Opts::new(
                "hodei_saga_compensation_total",
                "Total number of compensation steps executed",
            )
            .variable_labels(&["saga_type", "step_name"])
            .const_label("component", "saga"),
            &["saga_type", "step_name"],
        )?;
        registry.register(Box::new(saga_compensation_total.clone()))?;

        let saga_active = IntGaugeVec::new(
            Opts::new("hodei_saga_active", "Current number of active sagas")
                .variable_labels(&["saga_type"])
                .const_label("component", "saga"),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_active.clone()))?;

        let saga_step_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "hodei_saga_step_duration_seconds",
                "Saga step execution duration in seconds",
            )
            .variable_labels(&["saga_type", "step_name"])
            .const_label("component", "saga")
            .buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
            &["saga_type", "step_name"],
        )?;
        registry.register(Box::new(saga_step_duration_seconds.clone()))?;

        Ok(Self {
            saga_started: Arc::new(saga_started),
            saga_completed: Arc::new(saga_completed),
            saga_compensated: Arc::new(saga_compensated),
            saga_failed: Arc::new(saga_failed),
            saga_duration_seconds: Arc::new(saga_duration_seconds),
            saga_compensation_total: Arc::new(saga_compensation_total),
            saga_active: Arc::new(saga_active),
            saga_step_duration_seconds: Arc::new(saga_step_duration_seconds),
            registry: registry.clone(),
        })
    }

    /// Get the underlying registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

#[async_trait::async_trait]
impl hodei_server_domain::saga::SagaMetrics for PrometheusSagaMetrics {
    async fn record_started(&self, saga_type: hodei_server_domain::saga::SagaType) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_started.with_label_values(&[&saga_type_str]).inc();
    }

    async fn record_completed(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        duration_secs: f64,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_completed
            .with_label_values(&[&saga_type_str])
            .inc();
        self.saga_duration_seconds
            .with_label_values(&[&saga_type_str])
            .observe(duration_secs);
    }

    async fn record_compensated(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        compensation_step: &str,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_compensated
            .with_label_values(&[&saga_type_str, compensation_step])
            .inc();
    }

    async fn record_failed(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        error_type: &str,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_failed
            .with_label_values(&[&saga_type_str, error_type])
            .inc();
    }

    async fn record_compensation(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        step_name: &str,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_compensation_total
            .with_label_values(&[&saga_type_str, step_name])
            .inc();
    }

    async fn increment_active(&self, saga_type: hodei_server_domain::saga::SagaType) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_active.with_label_values(&[&saga_type_str]).inc();
    }

    async fn decrement_active(&self, saga_type: hodei_server_domain::saga::SagaType) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_active.with_label_values(&[&saga_type_str]).dec();
    }

    async fn record_step_duration(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        step_name: &str,
        duration_secs: f64,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.saga_step_duration_seconds
            .with_label_values(&[&saga_type_str, step_name])
            .observe(duration_secs);
    }
}

/// Convert SagaType to string for Prometheus labels
fn saga_type_str(saga_type: &hodei_server_domain::saga::SagaType) -> String {
    match saga_type {
        hodei_server_domain::saga::SagaType::Provisioning => "provisioning".to_string(),
        hodei_server_domain::saga::SagaType::Execution => "execution".to_string(),
        hodei_server_domain::saga::SagaType::Recovery => "recovery".to_string(),
    }
}

/// Internal saga metrics for MetricsRegistry (not a trait implementation)
/// EPIC-45 Gap 3: Internal metrics for MetricsRegistry
struct SagaMetricsInternal {
    /// Total number of saga executions started
    pub saga_started: IntCounterVec,
    /// Total number of saga executions completed successfully
    pub saga_completed: IntCounterVec,
    /// Total number of saga executions that required compensation
    pub saga_compensated: IntCounterVec,
    /// Total number of saga executions that failed
    pub saga_failed: IntCounterVec,
    /// Saga execution duration histogram
    pub saga_duration_seconds: HistogramVec,
    /// Number of compensations executed
    pub saga_compensation_total: IntCounterVec,
    /// Current number of active sagas
    pub saga_active: IntGaugeVec,
    /// Saga step execution latency
    pub saga_step_duration_seconds: HistogramVec,
}

impl SagaMetricsInternal {
    fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let saga_started = IntCounterVec::new(
            Opts::new(
                "hodei_saga_started_total",
                "Total number of saga executions started",
            )
            .variable_labels(&["saga_type"])
            .const_label("component", "saga"),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_started.clone()))?;

        let saga_completed = IntCounterVec::new(
            Opts::new(
                "hodei_saga_completed_total",
                "Total number of saga executions completed successfully",
            )
            .variable_labels(&["saga_type"])
            .const_label("component", "saga"),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_completed.clone()))?;

        let saga_compensated = IntCounterVec::new(
            Opts::new(
                "hodei_saga_compensated_total",
                "Total number of saga executions that required compensation",
            )
            .variable_labels(&["saga_type", "compensation_step"])
            .const_label("component", "saga"),
            &["saga_type", "compensation_step"],
        )?;
        registry.register(Box::new(saga_compensated.clone()))?;

        let saga_failed = IntCounterVec::new(
            Opts::new(
                "hodei_saga_failed_total",
                "Total number of saga executions that failed",
            )
            .variable_labels(&["saga_type", "error_type"])
            .const_label("component", "saga"),
            &["saga_type", "error_type"],
        )?;
        registry.register(Box::new(saga_failed.clone()))?;

        let saga_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "hodei_saga_duration_seconds",
                "Saga execution duration in seconds",
            )
            .variable_labels(&["saga_type"])
            .const_label("component", "saga")
            .buckets(vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0]),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_duration_seconds.clone()))?;

        let saga_compensation_total = IntCounterVec::new(
            Opts::new(
                "hodei_saga_compensation_total",
                "Total number of compensation steps executed",
            )
            .variable_labels(&["saga_type", "step_name"])
            .const_label("component", "saga"),
            &["saga_type", "step_name"],
        )?;
        registry.register(Box::new(saga_compensation_total.clone()))?;

        let saga_active = IntGaugeVec::new(
            Opts::new("hodei_saga_active", "Current number of active sagas")
                .variable_labels(&["saga_type"])
                .const_label("component", "saga"),
            &["saga_type"],
        )?;
        registry.register(Box::new(saga_active.clone()))?;

        let saga_step_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "hodei_saga_step_duration_seconds",
                "Saga step execution duration in seconds",
            )
            .variable_labels(&["saga_type", "step_name"])
            .const_label("component", "saga")
            .buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
            &["saga_type", "step_name"],
        )?;
        registry.register(Box::new(saga_step_duration_seconds.clone()))?;

        Ok(Self {
            saga_started,
            saga_completed,
            saga_compensated,
            saga_failed,
            saga_duration_seconds,
            saga_compensation_total,
            saga_active,
            saga_step_duration_seconds,
        })
    }
}

/// Wrapper for saga metrics compatible with domain trait
/// EPIC-45 Gap 3: Adapter for MetricsRegistry to domain trait
#[derive(Clone)]
pub struct SagaMetricsAdapter {
    metrics: Arc<SagaMetricsInternal>,
}

impl SagaMetricsAdapter {
    /// Create a new saga metrics adapter
    pub fn new(metrics: Arc<SagaMetricsInternal>) -> Self {
        Self { metrics }
    }
}

#[async_trait::async_trait]
impl hodei_server_domain::saga::SagaMetrics for SagaMetricsAdapter {
    async fn record_started(&self, saga_type: hodei_server_domain::saga::SagaType) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_started
            .with_label_values(&[&saga_type_str])
            .inc();
    }

    async fn record_completed(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        duration_secs: f64,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_completed
            .with_label_values(&[&saga_type_str])
            .inc();
        self.metrics
            .saga_duration_seconds
            .with_label_values(&[&saga_type_str])
            .observe(duration_secs);
    }

    async fn record_compensated(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        compensation_step: &str,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_compensated
            .with_label_values(&[&saga_type_str, compensation_step])
            .inc();
    }

    async fn record_failed(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        error_type: &str,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_failed
            .with_label_values(&[&saga_type_str, error_type])
            .inc();
    }

    async fn record_compensation(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        step_name: &str,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_compensation_total
            .with_label_values(&[&saga_type_str, step_name])
            .inc();
    }

    async fn increment_active(&self, saga_type: hodei_server_domain::saga::SagaType) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_active
            .with_label_values(&[&saga_type_str])
            .inc();
    }

    async fn decrement_active(&self, saga_type: hodei_server_domain::saga::SagaType) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_active
            .with_label_values(&[&saga_type_str])
            .dec();
    }

    async fn record_step_duration(
        &self,
        saga_type: hodei_server_domain::saga::SagaType,
        step_name: &str,
        duration_secs: f64,
    ) {
        let saga_type_str = saga_type_str(&saga_type);
        self.metrics
            .saga_step_duration_seconds
            .with_label_values(&[&saga_type_str, step_name])
            .observe(duration_secs);
    }
}

/// Helper macros for recording metrics

/// Record a business metric
#[macro_export]
macro_rules! record_business_metric {
    ($metric:ident, $($labels:expr),*) => {
        METRICS_REGISTRY.business().$metric.with_label_values(&[$($labels),*]).inc();
    };
    ($metric:ident, value: $val:expr, $($labels:expr),*) => {
        METRICS_REGISTRY.business().$metric.with_label_values(&[$($labels),*]).inc_by($val);
    };
}

/// Record a system metric
#[macro_export]
macro_rules! record_system_metric {
    ($metric:ident, $($labels:expr),*) => {
        METRICS_REGISTRY.system().$metric.with_label_values(&[$($labels),*]).inc();
    };
}

/// Record a performance metric
#[macro_export]
macro_rules! record_performance_metric {
    ($metric:ident, $($labels:expr),*) => {
        METRICS_REGISTRY.performance().$metric.with_label_values(&[$($labels),*]).inc();
    };
    ($metric:ident, value: $val:expr, $($labels:expr),*) => {
        METRICS_REGISTRY.performance().$metric.with_label_values(&[$($labels),*]).set($val);
    };
}

/// Record a security metric
#[macro_export]
macro_rules! record_security_metric {
    ($metric:ident, $($labels:expr),*) => {
        METRICS_REGISTRY.security().$metric.with_label_values(&[$($labels),*]).inc();
    };
    ($metric:ident, value: $val:expr, $($labels:expr),*) => {
        METRICS_REGISTRY.security().$metric.with_label_values(&[$($labels),*]).set($val);
    };
}

/// Record a saga metric
/// EPIC-45 Gap 3: Macro for saga observability metrics
#[macro_export]
macro_rules! record_saga_metric {
    ($metric:ident, $($labels:expr),*) => {
        METRICS_REGISTRY.saga().$metric.with_label_values(&[$($labels),*]).inc();
    };
    ($metric:ident, value: $val:expr, $($labels:expr),*) => {
        METRICS_REGISTRY.saga().$metric.with_label_values(&[$($labels),*]).observe($val);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        assert!(registry.is_ok());
    }

    #[test]
    fn test_metric_recording() {
        let _registry = MetricsRegistry::new().unwrap();
        // Test recording metrics would require actual metric updates
        // These are just structural tests
    }
}
