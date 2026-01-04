//! Observability Module - OpenTelemetry, Metrics, and Tracing
//!
//! Provides distributed tracing with OpenTelemetry, correlation ID propagation,
//! and Prometheus metrics for the Hodei Jobs platform.
//!
//! EPIC-43: Sprint 5 - Observabilidad (Trazabilidad)
//! US-EDA-501: Integrar OpenTelemetry
//! US-EDA-502: Propagar correlation_id a headers NATS
//! US-EDA-503: Dashboard de trazabilidad
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Observability Layer                           │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
//! │  │ OpenTelemetry│  │   Metrics    │  │  CorrelationId       │  │
//! │  │   Tracer     │  │  Registry    │  │  Propagation         │  │
//! │  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │
//! │         │                 │                     │               │
//! │         ▼                 ▼                     ▼               │
//! │  ┌──────────────────────────────────────────────────────────┐  │
//! │  │                    NATS Headers                           │  │
//! │  │  - traceparent (W3C Trace Context)                       │  │
//! │  │  - correlation-id (Hodei custom header)                  │  │
//! │  └──────────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

pub mod correlation;
pub mod metrics;
pub mod tracing;

/// Configuration for observability components
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// OpenTelemetry endpoint (OTLP exporter)
    pub otlp_endpoint: String,

    /// Service name for tracing
    pub service_name: String,

    /// Environment (production, development, etc.)
    pub environment: String,

    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,

    /// Whether to export traces
    pub enable_tracing: bool,

    /// Whether to collect metrics
    pub enable_metrics: bool,

    /// Prometheus metrics port
    pub metrics_port: u16,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: "http://localhost:4317".to_string(),
            service_name: "hodei-jobs".to_string(),
            environment: "development".to_string(),
            sampling_ratio: 1.0,
            enable_tracing: true,
            enable_metrics: true,
            metrics_port: 9090,
        }
    }
}

/// Result of observability initialization
#[derive(Debug)]
pub struct ObservabilityResult {
    /// Whether tracing was initialized
    pub tracing_enabled: bool,

    /// Whether metrics were initialized
    pub metrics_enabled: bool,

    /// Any errors that occurred
    pub errors: Vec<String>,
}

impl ObservabilityResult {
    pub fn new() -> Self {
        Self {
            tracing_enabled: false,
            metrics_enabled: false,
            errors: Vec::new(),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.tracing_enabled || self.metrics_enabled
    }
}

// Re-exports for convenience
pub use correlation::{
    CORRELATION_ID_HEADER, CorrelationContext, NatsHeaders, TRACE_PARENT_HEADER,
    TRACE_STATE_HEADER, context_to_headers, create_event_headers, extract_context_from_headers,
    extract_correlation_id_from_event,
};
pub use metrics::{GrpcMetrics, JobMetrics, MetricsRegistry, ObservabilityMetrics, WorkerMetrics};
pub use tracing::{
    TracingConfig, TracingResult, extract_trace_context, init_tracing, inject_trace_context,
    shutdown_tracing,
};
