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

use std::fmt;

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
            let trace_id = trace_parts.get(0).map(|s| s.to_string());
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
        let mut log = tracing::info_span!(
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
