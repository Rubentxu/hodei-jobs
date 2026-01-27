//! # Telemetry Module
//!
//! Tracing integration for saga-engine workflows.
//! Provides span and event recording for workflow execution,
//! activity execution, and compensation operations.
//!
//! ## Usage
//!
//! ```rust
//! use saga_engine_core::telemetry::{init_telemetry, SagaTelemetry};
//!
//! // Initialize telemetry at application startup
//! let _guard = init_telemetry("saga-engine", "1.0.0");
//!
//! // Use in workflows via WorkflowContext
//! ctx.add_trace_id("request_id", "abc123");
//! ```

#![allow(unexpected_cfgs)]

use std::collections::HashMap;
use std::time::Duration;
use tracing::{Level, Span, debug, error, info, span};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

/// Configuration for telemetry initialization
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name for tracing
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Log level filter
    pub log_level: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "saga-engine".to_string(),
            service_version: "1.0.0".to_string(),
            log_level: "INFO".to_string(),
        }
    }
}

/// Telemetry guard - must be kept alive for tracing to work
pub struct TelemetryGuard;

impl TelemetryGuard {
    /// Shutdown telemetry
    pub fn shutdown(self) {}
}

/// Initialize tracing for saga-engine
pub fn init_telemetry(config: &TelemetryConfig) -> TelemetryGuard {
    // Initialize with env filter
    let env_filter = EnvFilter::new(&config.log_level);

    Registry::default()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    TelemetryGuard
}

/// SagaTelemetry trait for workflow instrumentation
///
/// This trait provides methods for adding spans and events to saga
/// workflow traces.
pub trait SagaTelemetry: Send + Sync {
    /// Record workflow start
    fn on_workflow_start(&self, saga_type: &str, execution_id: &str);

    /// Record workflow completion
    fn on_workflow_complete(&self, saga_type: &str, execution_id: &str, duration: Duration);

    /// Record workflow failure
    fn on_workflow_failure(&self, saga_type: &str, execution_id: &str, error: &str);

    /// Record step execution
    fn on_step_start(&self, saga_type: &str, execution_id: &str, step: &str);

    /// Record step completion
    fn on_step_complete(&self, saga_type: &str, execution_id: &str, step: &str);

    /// Record step failure
    fn on_step_failure(&self, saga_type: &str, execution_id: &str, step: &str, error: &str);

    /// Record compensation action
    fn on_compensation_start(&self, saga_type: &str, execution_id: &str, step: &str);

    /// Record compensation completion
    fn on_compensation_complete(&self, saga_type: &str, execution_id: &str, step: &str);

    /// Record compensation failure
    fn on_compensation_failure(&self, saga_type: &str, execution_id: &str, step: &str, error: &str);

    /// Add custom attribute to current span
    fn add_attribute(&self, key: &str, value: &str);

    /// Add custom event to current span
    fn add_event(&self, name: &str, attributes: HashMap<String, String>);
}

/// Default tracing-based implementation
#[derive(Debug, Default)]
pub struct DefaultSagaTelemetry;

impl DefaultSagaTelemetry {
    pub fn new() -> Self {
        Self
    }

    /// Create a span for the workflow
    fn workflow_span(&self, saga_type: &str, execution_id: &str, action: &str) -> Span {
        span!(
            Level::INFO,
            "saga",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.action = action,
        )
    }

    /// Create a span for a step
    fn step_span(&self, saga_type: &str, execution_id: &str, step: &str, action: &str) -> Span {
        span!(
            Level::INFO,
            "saga.step",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step,
            saga.action = action,
        )
    }

    /// Create a span for compensation
    fn compensation_span(
        &self,
        saga_type: &str,
        execution_id: &str,
        step: &str,
        action: &str,
    ) -> Span {
        span!(
            Level::INFO,
            "saga.compensation",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step,
            saga.compensation = action,
        )
    }
}

impl SagaTelemetry for DefaultSagaTelemetry {
    fn on_workflow_start(&self, saga_type: &str, execution_id: &str) {
        let _span = self.workflow_span(saga_type, execution_id, "start");
        info!(
            saga_type = saga_type,
            execution_id = execution_id,
            "Workflow started"
        );
    }

    fn on_workflow_complete(&self, saga_type: &str, execution_id: &str, _duration: Duration) {
        let _span = self.workflow_span(saga_type, execution_id, "complete");
        info!(
            saga_type = saga_type,
            execution_id = execution_id,
            "Workflow completed successfully"
        );
    }

    fn on_workflow_failure(&self, saga_type: &str, execution_id: &str, error: &str) {
        let _span = self.workflow_span(saga_type, execution_id, "failure");
        error!(
            saga_type = saga_type,
            execution_id = execution_id,
            error = error,
            "Workflow failed"
        );
    }

    fn on_step_start(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = self.step_span(saga_type, execution_id, step, "start");
        debug!(
            saga_type = saga_type,
            execution_id = execution_id,
            step = step,
            "Step started"
        );
    }

    fn on_step_complete(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = self.step_span(saga_type, execution_id, step, "complete");
        debug!(
            saga_type = saga_type,
            execution_id = execution_id,
            step = step,
            "Step completed"
        );
    }

    fn on_step_failure(&self, saga_type: &str, execution_id: &str, step: &str, error: &str) {
        let _span = self.step_span(saga_type, execution_id, step, "failure");
        error!(
            saga_type = saga_type,
            execution_id = execution_id,
            step = step,
            error = error,
            "Step failed"
        );
    }

    fn on_compensation_start(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = self.compensation_span(saga_type, execution_id, step, "start");
        debug!(
            saga_type = saga_type,
            execution_id = execution_id,
            step = step,
            "Compensation started"
        );
    }

    fn on_compensation_complete(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = self.compensation_span(saga_type, execution_id, step, "complete");
        debug!(
            saga_type = saga_type,
            execution_id = execution_id,
            step = step,
            "Compensation completed"
        );
    }

    fn on_compensation_failure(
        &self,
        saga_type: &str,
        execution_id: &str,
        step: &str,
        error: &str,
    ) {
        let _span = self.compensation_span(saga_type, execution_id, step, "failure");
        error!(
            saga_type = saga_type,
            execution_id = execution_id,
            step = step,
            error = error,
            "Compensation failed"
        );
    }

    fn add_attribute(&self, key: &str, value: &str) {
        Span::current().record(key, value);
    }

    fn add_event(&self, name: &str, attributes: HashMap<String, String>) {
        tracing::info!(event = name, ?attributes);
    }
}

/// OpenTelemetry-based implementation (optional, requires feature flag)
#[cfg(feature = "opentelemetry")]
pub struct OpenTelemetrySagaTelemetry;

#[cfg(feature = "opentelemetry")]
impl OpenTelemetrySagaTelemetry {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "opentelemetry")]
impl SagaTelemetry for OpenTelemetrySagaTelemetry {
    fn on_workflow_start(&self, saga_type: &str, execution_id: &str) {
        let _span = span!(
            Level::INFO,
            "saga",
            saga.type_name = saga_type,
            saga.execution_id = execution_id
        );
        info!(
            saga_type = saga_type,
            execution_id = execution_id,
            "Workflow started"
        );
    }

    fn on_workflow_complete(&self, saga_type: &str, execution_id: &str, _duration: Duration) {
        let _span = span!(
            Level::INFO,
            "saga",
            saga.type_name = saga_type,
            saga.execution_id = execution_id
        );
        info!(
            saga_type = saga_type,
            execution_id = execution_id,
            "Workflow completed"
        );
    }

    fn on_workflow_failure(&self, saga_type: &str, execution_id: &str, error: &str) {
        let _span = span!(
            Level::INFO,
            "saga",
            saga.type_name = saga_type,
            saga.execution_id = execution_id
        );
        error!(
            saga_type = saga_type,
            execution_id = execution_id,
            error = error,
            "Workflow failed"
        );
    }

    fn on_step_start(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = span!(
            Level::INFO,
            "saga.step",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step
        );
        debug!(step = step, "Step started");
    }

    fn on_step_complete(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = span!(
            Level::INFO,
            "saga.step",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step
        );
        debug!(step = step, "Step completed");
    }

    fn on_step_failure(&self, saga_type: &str, execution_id: &str, step: &str, error: &str) {
        let _span = span!(
            Level::INFO,
            "saga.step",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step
        );
        error!(step = step, error = error, "Step failed");
    }

    fn on_compensation_start(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = span!(
            Level::INFO,
            "saga.compensation",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step
        );
        debug!(step = step, "Compensation started");
    }

    fn on_compensation_complete(&self, saga_type: &str, execution_id: &str, step: &str) {
        let _span = span!(
            Level::INFO,
            "saga.compensation",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step
        );
        debug!(step = step, "Compensation completed");
    }

    fn on_compensation_failure(
        &self,
        saga_type: &str,
        execution_id: &str,
        step: &str,
        error: &str,
    ) {
        let _span = span!(
            Level::INFO,
            "saga.compensation",
            saga.type_name = saga_type,
            saga.execution_id = execution_id,
            saga.step = step
        );
        error!(step = step, error = error, "Compensation failed");
    }

    fn add_attribute(&self, key: &str, value: &str) {
        Span::current().record(key, value);
    }

    fn add_event(&self, name: &str, attributes: HashMap<String, String>) {
        tracing::info!(event = name, ?attributes);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_defaults() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "saga-engine");
        assert_eq!(config.service_version, "1.0.0");
        assert_eq!(config.log_level, "INFO");
    }

    #[test]
    fn test_telemetry_guard_shutdown() {
        let config = TelemetryConfig::default();
        let guard = init_telemetry(&config);
        guard.shutdown();
    }

    #[tokio::test]
    async fn test_saga_telemetry_default() {
        let telemetry = DefaultSagaTelemetry::new();

        // These should not panic
        telemetry.on_workflow_start("test_saga", "exec-123");
        telemetry.on_workflow_complete("test_saga", "exec-123", Duration::from_millis(100));
        telemetry.on_step_start("test_saga", "exec-123", "step_1");
        telemetry.on_step_complete("test_saga", "exec-123", "step_1");
        telemetry.on_step_failure("test_saga", "exec-123", "step_1", "error");

        let mut attrs = HashMap::new();
        attrs.insert("test_key".to_string(), "test_value".to_string());
        telemetry.add_event("test_event", attrs);
        telemetry.add_attribute("test_attr", "test_value");
    }

    #[tokio::test]
    async fn test_saga_telemetry_compensation() {
        let telemetry = DefaultSagaTelemetry::new();

        telemetry.on_compensation_start("test_saga", "exec-123", "step_1");
        telemetry.on_compensation_complete("test_saga", "exec-123", "step_1");
        telemetry.on_compensation_failure("test_saga", "exec-123", "step_1", "compensation error");
    }
}
