//! Telemetry Module
//!
//! Provides OpenTelemetry integration for distributed tracing and metrics
//! in the Hodei Jobs Platform.
//!
//! EPIC-54: OpenTelemetry Integration
//! - Span attributes for saga steps
//! - Context propagation for distributed traces
//! - Tracing integration for saga execution
//!
//! This module provides sagas with standardized telemetry attributes
//! and uses the `tracing` crate for span creation and recording.

/// Result type for telemetry operations
pub type TelemetryResult<T> = std::result::Result<T, TelemetryError>;

/// Errors that can occur during telemetry operations
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("Tracer initialization failed: {0}")]
    InitializationError(String),

    #[error("Span operation failed: {0}")]
    SpanError(String),

    #[error("Context propagation failed: {0}")]
    ContextError(String),
}

/// Saga span attributes for OpenTelemetry
///
/// Provides structured attributes for saga execution tracing.
/// These attributes are designed to be compatible with OpenTelemetry
/// semantic conventions for distributed tracing.
#[derive(Debug, Clone)]
pub struct SagaSpanAttributes {
    /// Saga type name (e.g., "provisioning", "execution", "cancellation")
    pub saga_type: String,
    /// Saga ID (unique identifier for this saga instance)
    pub saga_id: String,
    /// Current step name
    pub step_name: String,
    /// Job ID if applicable
    pub job_id: Option<String>,
    /// Provider ID if applicable
    pub provider_id: Option<String>,
    /// Worker ID if applicable
    pub worker_id: Option<String>,
    /// Step outcome ("success", "failure", "compensation")
    pub outcome: Option<String>,
    /// Error message if the step failed
    pub error_message: Option<String>,
}

impl SagaSpanAttributes {
    /// Creates new saga span attributes
    pub fn new(
        saga_type: impl Into<String>,
        saga_id: impl Into<String>,
        step_name: impl Into<String>,
    ) -> Self {
        Self {
            saga_type: saga_type.into(),
            saga_id: saga_id.into(),
            step_name: step_name.into(),
            job_id: None,
            provider_id: None,
            worker_id: None,
            outcome: None,
            error_message: None,
        }
    }

    /// Sets the job ID
    pub fn with_job_id(mut self, job_id: impl Into<String>) -> Self {
        self.job_id = Some(job_id.into());
        self
    }

    /// Sets the provider ID
    pub fn with_provider_id(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    /// Sets the worker ID
    pub fn with_worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = Some(worker_id.into());
        self
    }

    /// Sets the step outcome
    pub fn with_outcome(mut self, outcome: impl Into<String>) -> Self {
        self.outcome = Some(outcome.into());
        self
    }

    /// Sets the error message
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error_message = Some(error.into());
        self
    }
}

/// Telemetry context for saga execution
///
/// Carries trace context information for saga step propagation.
/// Supports W3C Trace Context format for distributed tracing.
#[derive(Clone, Debug)]
pub struct SagaTelemetryContext {
    /// W3C trace parent header value
    pub trace_parent: Option<String>,
    /// Trace ID for correlation
    pub trace_id: Option<String>,
    /// Span ID for parent linking
    pub parent_span_id: Option<String>,
}

impl Default for SagaTelemetryContext {
    fn default() -> Self {
        Self::new()
    }
}

impl SagaTelemetryContext {
    /// Creates a new telemetry context
    pub fn new() -> Self {
        Self {
            trace_parent: None,
            trace_id: None,
            parent_span_id: None,
        }
    }

    /// Creates a telemetry context from W3C trace parent header
    ///
    /// Parses the W3C Trace Context format: version-trace-id/span-id;flags
    /// and extracts trace and span IDs for propagation.
    ///
    /// # Arguments
    /// * `trace_parent` - W3C trace parent header value
    ///
    /// # Returns
    /// Telemetry context with extracted trace information
    pub fn from_trace_parent(trace_parent: &str) -> Self {
        // Parse W3C trace parent format: version-trace-id/span-id;flags
        // Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
        // Format breakdown: 00-<trace_id(32 hex)>/<span_id(16 hex)>;<flags(2 hex)>

        // First split by '-' to separate version from trace-id/span-id;flags
        let parts: Vec<&str> = trace_parent.splitn(2, '-').collect();

        if parts.len() != 2 {
            return Self::new();
        }

        // Check if it contains '/' (W3C format) or not (simplified format)
        let trace_and_span = parts[1];

        if trace_and_span.contains('/') {
            // W3C format: trace-id/span-id;flags
            let trace_split: Vec<&str> = trace_and_span.splitn(2, '/').collect();

            if trace_split.len() != 2 {
                return Self::new();
            }

            let trace_id = trace_split[0].to_string();

            // span_id is before the ';'
            let span_and_flags: Vec<&str> = trace_split[1].splitn(2, ';').collect();
            let span_id = span_and_flags[0].to_string();

            Self {
                trace_parent: Some(trace_parent.to_string()),
                trace_id: Some(trace_id),
                parent_span_id: Some(span_id),
            }
        } else {
            // Simplified format: version-trace-id-span-id-flags
            // Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
            let trace_parts: Vec<&str> = trace_and_span.splitn(3, '-').collect();

            // trace_id is parts[0], span_id is parts[1]
            let trace_id = trace_parts.first().map(|s| s.to_string());
            let span_id = trace_parts.get(1).map(|s| s.to_string());

            Self {
                trace_parent: Some(trace_parent.to_string()),
                trace_id,
                parent_span_id: span_id,
            }
        }
    }

    /// Returns the parent span ID
    pub fn parent_span_id(&self) -> Option<&str> {
        self.parent_span_id.as_deref()
    }

    /// Returns the trace ID
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }
}

/// Initializes the OpenTelemetry tracer with OTLP exporter
///
/// # Arguments
/// * `service_name` - Name of the service for tracing
/// * `endpoint` - OTLP endpoint for trace export (e.g., "http://localhost:4317")
///
/// # Returns
/// A guard that must be kept alive for tracing to work
#[cfg(feature = "lifecycle-management")]
pub fn init_telemetry(service_name: &str, endpoint: &str) -> TelemetryResult<impl Drop> {
    use opentelemetry_sdk::trace::Config;
    use opentelemetry_sdk::{self as sdk, resource::Resource};
    use std::collections::HashMap;

    // Build resource with service information
    let mut labels = HashMap::new();
    labels.insert("service.name".to_string(), service_name.to_string());
    labels.insert(
        "service.version".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );

    let resource = Resource::new(labels);

    // Create OTLP exporter with tonic transport
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint);

    // Configure tracer provider with batch processor for efficiency
    let tracer_provider = sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, sdk::runtime::Tokio)
        .with_config(Config::default().with_resource(resource))
        .build();

    // Set as global tracer provider
    let provider_guard = opentelemetry::global::set_tracer_provider(tracer_provider);

    // Also set up tracing-opentelemetry layer for integration with tracing
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    let tracer = opentelemetry::global::tracer(service_name);
    let otlp_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Registry::default()
        .with(otlp_layer)
        .try_init()
        .map_err(|e| TelemetryError::InitializationError(e.to_string()))?;

    Ok(provider_guard)
}

/// Telemetry utilities for saga execution
pub mod saga {
    use super::*;

    /// Records saga execution starting
    ///
    /// # Arguments
    /// * `saga_type` - Type of saga (e.g., "provisioning", "execution")
    /// * `saga_id` - Unique saga identifier
    #[inline]
    pub fn record_saga_started(saga_type: &str, saga_id: &str) {
        tracing::info!(
            saga.type = saga_type,
            saga.id = saga_id,
            event = "saga_started"
        );
    }

    /// Records saga step execution
    ///
    /// # Arguments
    /// * `saga_type` - Type of saga
    /// * `saga_id` - Saga identifier
    /// * `step_name` - Name of the current step
    /// * `attributes` - Additional span attributes
    #[inline]
    pub fn record_step_start(
        saga_type: &str,
        saga_id: &str,
        step_name: &str,
        attributes: Option<&SagaSpanAttributes>,
    ) {
        let log = tracing::info_span!(
            "saga.step",
            saga.type = saga_type,
            saga.id = saga_id,
            step.name = step_name,
            step.phase = "start"
        );

        if let Some(attrs) = attributes {
            if let Some(ref job_id) = attrs.job_id {
                log.record("job.id", job_id.as_str());
            }
            if let Some(ref provider_id) = attrs.provider_id {
                log.record("provider.id", provider_id.as_str());
            }
            if let Some(ref worker_id) = attrs.worker_id {
                log.record("worker.id", worker_id.as_str());
            }
        }

        let _guard = log.enter();
        tracing::debug!("Step {} started", step_name);
    }

    /// Records saga step completion
    ///
    /// # Arguments
    /// * `step_name` - Name of the completed step
    /// * `success` - Whether the step succeeded
    /// * `duration_ms` - Execution duration in milliseconds
    /// * `error` - Optional error message
    #[inline]
    pub fn record_step_completed(
        step_name: &str,
        success: bool,
        duration_ms: u64,
        error: Option<&str>,
    ) {
        let outcome = if success { "success" } else { "failure" };

        if success {
            tracing::info!(
                step.name = step_name,
                step.outcome = outcome,
                step.duration_ms = duration_ms,
                event = "step_completed"
            );
        } else {
            tracing::error!(
                step.name = step_name,
                step.outcome = outcome,
                step.duration_ms = duration_ms,
                error.message = error.unwrap_or("unknown"),
                event = "step_failed"
            );
        }
    }

    /// Records saga compensation event
    ///
    /// # Arguments
    /// * `saga_type` - Type of saga
    /// * `step_name` - Name of the compensating step
    /// * `succeeded` - Whether compensation succeeded
    #[inline]
    pub fn record_compensation(saga_type: &str, step_name: &str, succeeded: bool) {
        let outcome = if succeeded { "success" } else { "failure" };

        tracing::info!(
            saga.type = saga_type,
            saga.compensation.step = step_name,
            saga.compensation.outcome = outcome,
            event = "compensation"
        );
    }

    /// Records saga completion
    ///
    /// # Arguments
    /// * `saga_type` - Type of saga
    /// * `saga_id` - Saga identifier
    /// * `success` - Whether the saga completed successfully
    #[inline]
    pub fn record_saga_completed(saga_type: &str, saga_id: &str, success: bool) {
        let outcome = if success { "success" } else { "failure" };

        tracing::info!(
            saga.type = saga_type,
            saga.id = saga_id,
            saga.outcome = outcome,
            event = "saga_completed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saga_span_attributes_builder() {
        let attrs = SagaSpanAttributes::new("provisioning", "saga-123", "CreateInfrastructure")
            .with_job_id("job-456")
            .with_provider_id("docker-local");

        assert_eq!(attrs.saga_type, "provisioning");
        assert_eq!(attrs.saga_id, "saga-123");
        assert_eq!(attrs.step_name, "CreateInfrastructure");
        assert_eq!(attrs.job_id, Some("job-456".to_string()));
        assert_eq!(attrs.provider_id, Some("docker-local".to_string()));
        assert!(attrs.worker_id.is_none());
    }

    #[test]
    fn saga_span_attributes_with_all_fields() {
        let attrs = SagaSpanAttributes::new("provisioning", "saga-123", "TerminateWorker")
            .with_job_id("job-456")
            .with_provider_id("docker-local")
            .with_worker_id("worker-789")
            .with_outcome("failure")
            .with_error("timeout");

        assert_eq!(attrs.saga_type, "provisioning");
        assert_eq!(attrs.saga_id, "saga-123");
        assert_eq!(attrs.step_name, "TerminateWorker");
        assert_eq!(attrs.job_id, Some("job-456".to_string()));
        assert_eq!(attrs.provider_id, Some("docker-local".to_string()));
        assert_eq!(attrs.worker_id, Some("worker-789".to_string()));
        assert_eq!(attrs.outcome, Some("failure".to_string()));
        assert_eq!(attrs.error_message, Some("timeout".to_string()));
    }

    #[test]
    fn saga_telemetry_context_from_trace_parent() {
        let ctx = SagaTelemetryContext::from_trace_parent(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        );

        assert!(ctx.parent_span_id().is_some());
        assert_eq!(ctx.trace_id(), Some("0af7651916cd43dd8448eb211c80319c"));
        assert_eq!(ctx.parent_span_id(), Some("b7ad6b7169203331"));
    }

    #[test]
    fn saga_telemetry_context_default() {
        let ctx = SagaTelemetryContext::new();
        assert!(ctx.parent_span_id().is_none());
        assert!(ctx.trace_id().is_none());
    }
}

// ============================================================================
// Advanced Metrics for Saga, Outbox, and Command Processing
// ============================================================================

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Histogram-style metric using atomic bucketing
#[derive(Clone, Default)]
pub struct HistogramMetric {
    counts: Arc<Mutex<Vec<u64>>>,
    total: Arc<std::sync::atomic::AtomicU64>,
    sum: Arc<std::sync::atomic::AtomicU64>,
    min: Arc<std::sync::atomic::AtomicU64>,
    max: Arc<std::sync::atomic::AtomicU64>,
}

impl HistogramMetric {
    pub fn new(bucket_count: usize) -> Self {
        Self {
            counts: Arc::new(Mutex::new(vec![0; bucket_count])),
            total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            sum: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            min: Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX)),
            max: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn observe(&self, value: u64, bucket_size: u64) {
        let bucket_count = self.counts.blocking_lock().len();
        let bucket = (value / bucket_size).min(bucket_count as u64 - 1) as usize;
        self.counts.blocking_lock()[bucket] += 1;

        self.total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.sum
            .fetch_add(value, std::sync::atomic::Ordering::Relaxed);

        if value < 1_000_000_000 {
            self.min
                .fetch_min(value, std::sync::atomic::Ordering::Relaxed);
        }
        self.max
            .fetch_max(value, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> HistogramStats {
        let counts = self.counts.blocking_lock().clone();
        let total = self.total.load(std::sync::atomic::Ordering::Relaxed);
        let sum = self.sum.load(std::sync::atomic::Ordering::Relaxed);
        let min = self.min.load(std::sync::atomic::Ordering::Relaxed);
        let max = self.max.load(std::sync::atomic::Ordering::Relaxed);

        HistogramStats {
            total,
            sum,
            min: if min == u64::MAX { 0 } else { min },
            max,
            buckets: counts,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistogramStats {
    pub total: u64,
    pub sum: u64,
    pub min: u64,
    pub max: u64,
    pub buckets: Vec<u64>,
}

impl HistogramStats {
    pub fn percentile(&self, p: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let target = (self.total as f64 * p / 100.0).ceil() as u64;
        let mut cumulative = 0;
        for (i, &count) in self.buckets.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return (i as u64 * 100);
            }
        }
        self.max
    }
}

/// Saga Execution Metrics
#[derive(Clone, Default)]
pub struct SagaMetrics {
    pub execution_duration_ms: HistogramMetric,
    pub active_sagas: Arc<std::sync::atomic::AtomicU64>,
    pub completed_total: Arc<std::sync::atomic::AtomicU64>,
    pub failed_total: Arc<std::sync::atomic::AtomicU64>,
    pub timed_out_total: Arc<std::sync::atomic::AtomicU64>,
    pub steps_completed: Arc<std::sync::atomic::AtomicU64>,
    pub step_failures: Arc<std::sync::Mutex<Vec<(String, u64)>>>,
    pub step_latency_ms: HistogramMetric,
}

impl SagaMetrics {
    pub fn new() -> Self {
        Self {
            execution_duration_ms: HistogramMetric::new(20),
            active_sagas: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            completed_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            failed_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            timed_out_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            steps_completed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            step_failures: Arc::new(std::sync::Mutex::new(Vec::new())),
            step_latency_ms: HistogramMetric::new(20),
        }
    }

    pub fn record_execution_start(&self) {
        self.active_sagas
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_execution_complete(&self, duration_ms: u64, success: bool, timed_out: bool) {
        self.active_sagas
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        self.execution_duration_ms.observe(duration_ms, 100);

        if timed_out {
            self.timed_out_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else if success {
            self.completed_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.failed_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub fn record_step_complete(&self, _step_name: &str, latency_ms: u64) {
        self.steps_completed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.step_latency_ms.observe(latency_ms, 50);
    }

    pub fn record_step_failure(&self, step_name: &str) {
        let mut failures = self.step_failures.lock().unwrap();
        if let Some((_, count)) = failures.iter_mut().find(|(name, _)| name == step_name) {
            *count += 1;
        } else {
            failures.push((step_name.to_string(), 1));
        }
    }

    pub fn get_summary(&self) -> SagaMetricsSummary {
        let duration_stats = self.execution_duration_ms.get_stats();
        let step_stats = self.step_latency_ms.get_stats();
        let step_failures = self.step_failures.lock().unwrap().clone();

        SagaMetricsSummary {
            active_sagas: self.active_sagas.load(std::sync::atomic::Ordering::Relaxed),
            completed_total: self
                .completed_total
                .load(std::sync::atomic::Ordering::Relaxed),
            failed_total: self.failed_total.load(std::sync::atomic::Ordering::Relaxed),
            timed_out_total: self
                .timed_out_total
                .load(std::sync::atomic::Ordering::Relaxed),
            steps_completed: self
                .steps_completed
                .load(std::sync::atomic::Ordering::Relaxed),
            execution_p50_ms: duration_stats.percentile(50.0),
            execution_p95_ms: duration_stats.percentile(95.0),
            execution_p99_ms: duration_stats.percentile(99.0),
            step_latency_p50_ms: step_stats.percentile(50.0),
            step_latency_p95_ms: step_stats.percentile(95.0),
            step_failures_by_name: step_failures,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SagaMetricsSummary {
    pub active_sagas: u64,
    pub completed_total: u64,
    pub failed_total: u64,
    pub timed_out_total: u64,
    pub steps_completed: u64,
    pub execution_p50_ms: u64,
    pub execution_p95_ms: u64,
    pub execution_p99_ms: u64,
    pub step_latency_p50_ms: u64,
    pub step_latency_p95_ms: u64,
    pub step_failures_by_name: Vec<(String, u64)>,
}

/// Outbox Metrics
#[derive(Clone, Default)]
pub struct OutboxMetrics {
    pub pending_count: Arc<std::sync::atomic::AtomicU64>,
    pub processing_lag_seconds: HistogramMetric,
    pub published_total: Arc<std::sync::atomic::AtomicU64>,
    pub publish_failures: Arc<std::sync::atomic::AtomicU64>,
    pub batch_size: HistogramMetric,
    pub relay_cycle_duration_ms: HistogramMetric,
    pub last_publish_at: Arc<Mutex<Option<Instant>>>,
}

impl OutboxMetrics {
    pub fn new() -> Self {
        Self {
            pending_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            processing_lag_seconds: HistogramMetric::new(30),
            published_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            publish_failures: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            batch_size: HistogramMetric::new(10),
            relay_cycle_duration_ms: HistogramMetric::new(20),
            last_publish_at: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_pending_count(&self, count: u64) {
        self.pending_count
            .store(count, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_publish(&self, lag_seconds: u64, batch_size: u64) {
        self.processing_lag_seconds.observe(lag_seconds, 1);
        self.batch_size.observe(batch_size, 10);
        self.published_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *self.last_publish_at.blocking_lock() = Some(Instant::now());
    }

    pub fn record_publish_failure(&self) {
        self.publish_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_relay_cycle(&self, duration_ms: u64) {
        self.relay_cycle_duration_ms.observe(duration_ms, 50);
    }

    pub fn get_summary(&self) -> OutboxMetricsSummary {
        let lag_stats = self.processing_lag_seconds.get_stats();
        let batch_stats = self.batch_size.get_stats();
        let cycle_stats = self.relay_cycle_duration_ms.get_stats();

        OutboxMetricsSummary {
            pending_count: self
                .pending_count
                .load(std::sync::atomic::Ordering::Relaxed),
            published_total: self
                .published_total
                .load(std::sync::atomic::Ordering::Relaxed),
            publish_failures: self
                .publish_failures
                .load(std::sync::atomic::Ordering::Relaxed),
            lag_p50_seconds: lag_stats.percentile(50.0),
            lag_p95_seconds: lag_stats.percentile(95.0),
            lag_p99_seconds: lag_stats.percentile(99.0),
            avg_batch_size: if batch_stats.total > 0 {
                batch_stats.sum / batch_stats.total
            } else {
                0
            },
            relay_cycle_p50_ms: cycle_stats.percentile(50.0),
            last_publish_seconds_ago: {
                let last = *self.last_publish_at.blocking_lock();
                last.map(|i| i.elapsed().as_secs()).unwrap_or(0)
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutboxMetricsSummary {
    pub pending_count: u64,
    pub published_total: u64,
    pub publish_failures: u64,
    pub lag_p50_seconds: u64,
    pub lag_p95_seconds: u64,
    pub lag_p99_seconds: u64,
    pub avg_batch_size: u64,
    pub relay_cycle_p50_ms: u64,
    pub last_publish_seconds_ago: u64,
}

/// Command Dispatch Metrics
#[derive(Clone, Default)]
pub struct CommandMetrics {
    pub dispatch_latency_ms: HistogramMetric,
    pub dispatched_total: Arc<std::sync::atomic::AtomicU64>,
    pub completed_total: Arc<std::sync::atomic::AtomicU64>,
    pub failed_total: Arc<std::sync::atomic::AtomicU64>,
    pub in_flight: Arc<std::sync::atomic::AtomicU64>,
    pub retry_count: HistogramMetric,
}

impl CommandMetrics {
    pub fn new() -> Self {
        Self {
            dispatch_latency_ms: HistogramMetric::new(30),
            dispatched_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            completed_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            failed_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            in_flight: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            retry_count: HistogramMetric::new(10),
        }
    }

    pub fn record_dispatch(&self, latency_ms: u64) {
        self.dispatch_latency_ms.observe(latency_ms, 100);
        self.dispatched_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.in_flight
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_complete(&self, retries: u64) {
        self.in_flight
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        self.completed_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.retry_count.observe(retries, 1);
    }

    pub fn record_failure(&self) {
        self.in_flight
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        self.failed_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_summary(&self) -> CommandMetricsSummary {
        let dispatch_stats = self.dispatch_latency_ms.get_stats();
        let retry_stats = self.retry_count.get_stats();

        CommandMetricsSummary {
            dispatched_total: self
                .dispatched_total
                .load(std::sync::atomic::Ordering::Relaxed),
            completed_total: self
                .completed_total
                .load(std::sync::atomic::Ordering::Relaxed),
            failed_total: self.failed_total.load(std::sync::atomic::Ordering::Relaxed),
            in_flight: self.in_flight.load(std::sync::atomic::Ordering::Relaxed),
            dispatch_p50_ms: dispatch_stats.percentile(50.0),
            dispatch_p95_ms: dispatch_stats.percentile(95.0),
            dispatch_p99_ms: dispatch_stats.percentile(99.0),
            avg_retries: if retry_stats.total > 0 {
                retry_stats.sum / retry_stats.total
            } else {
                0
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandMetricsSummary {
    pub dispatched_total: u64,
    pub completed_total: u64,
    pub failed_total: u64,
    pub in_flight: u64,
    pub dispatch_p50_ms: u64,
    pub dispatch_p95_ms: u64,
    pub dispatch_p99_ms: u64,
    pub avg_retries: u64,
}

/// Combined System Telemetry
#[derive(Clone, Default)]
pub struct SystemTelemetry {
    pub saga: SagaMetrics,
    pub outbox: OutboxMetrics,
    pub command: CommandMetrics,
}

impl SystemTelemetry {
    pub fn new() -> Self {
        Self {
            saga: SagaMetrics::new(),
            outbox: OutboxMetrics::new(),
            command: CommandMetrics::new(),
        }
    }

    pub fn get_full_summary(&self) -> SystemTelemetrySummary {
        SystemTelemetrySummary {
            saga: self.saga.get_summary(),
            outbox: self.outbox.get_summary(),
            command: self.command.get_summary(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemTelemetrySummary {
    pub saga: SagaMetricsSummary,
    pub outbox: OutboxMetricsSummary,
    pub command: CommandMetricsSummary,
}
