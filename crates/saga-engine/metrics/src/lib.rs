//! # saga-engine-metrics
//!
//! Prometheus metrics for saga-engine v4 workflows and activities.

use prometheus::{Encoder, TextEncoder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Prometheus metrics implementation for saga-engine
#[derive(Clone)]
pub struct PrometheusMetrics {
    /// Prometheus registry
    registry: prometheus::Registry,

    // Workflow metrics
    workflow_started: prometheus::IntCounterVec,
    workflow_completed: prometheus::IntCounterVec,
    workflow_failed: prometheus::IntCounterVec,
    workflow_duration: prometheus::HistogramVec,
    workflow_active: prometheus::IntGaugeVec,

    // Activity metrics
    activity_started: prometheus::IntCounterVec,
    activity_completed: prometheus::IntCounterVec,
    activity_failed: prometheus::IntCounterVec,
    activity_duration: prometheus::HistogramVec,

    // Replay metrics
    replay_duration: prometheus::HistogramVec,
    replay_events: prometheus::HistogramVec,

    // Snapshot metrics
    snapshot_taken: prometheus::IntCounterVec,
    snapshot_size: prometheus::HistogramVec,
}

impl PrometheusMetrics {
    /// Create new Prometheus metrics with default buckets
    pub fn new() -> Result<Self, prometheus::Error> {
        Self::with_buckets(
            vec![0.1, 0.5, 1.0, 5.0, 30.0, 60.0, 300.0, 1800.0],
            vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 30.0],
        )
    }

    /// Create new Prometheus metrics with custom buckets
    pub fn with_buckets(
        workflow_buckets: Vec<f64>,
        activity_buckets: Vec<f64>,
    ) -> Result<Self, prometheus::Error> {
        let registry = prometheus::Registry::new();

        // Workflow metrics
        let workflow_started = prometheus::IntCounterVec::new(
            prometheus::opts!(
                "saga_workflow_started_total",
                "Total workflows started across all types"
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_started.clone()))?;

        let workflow_completed = prometheus::IntCounterVec::new(
            prometheus::opts!(
                "saga_workflow_completed_total",
                "Total workflows completed successfully"
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_completed.clone()))?;

        let workflow_failed = prometheus::IntCounterVec::new(
            prometheus::opts!("saga_workflow_failed_total", "Total workflows that failed"),
            &["workflow_type", "error_type"],
        )?;
        registry.register(Box::new(workflow_failed.clone()))?;

        let workflow_duration = prometheus::HistogramVec::new(
            prometheus::histogram_opts!(
                "saga_workflow_duration_seconds",
                "Workflow execution duration in seconds",
                workflow_buckets
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_duration.clone()))?;

        let workflow_active = prometheus::IntGaugeVec::new(
            prometheus::opts!("saga_workflow_active", "Currently active workflows"),
            &["workflow_type"],
        )?;
        registry.register(Box::new(workflow_active.clone()))?;

        // Activity metrics
        let activity_started = prometheus::IntCounterVec::new(
            prometheus::opts!("saga_activity_started_total", "Total activities started"),
            &["activity_type"],
        )?;
        registry.register(Box::new(activity_started.clone()))?;

        let activity_completed = prometheus::IntCounterVec::new(
            prometheus::opts!(
                "saga_activity_completed_total",
                "Total activities completed successfully"
            ),
            &["activity_type"],
        )?;
        registry.register(Box::new(activity_completed.clone()))?;

        let activity_failed = prometheus::IntCounterVec::new(
            prometheus::opts!("saga_activity_failed_total", "Total activities that failed"),
            &["activity_type", "error_type"],
        )?;
        registry.register(Box::new(activity_failed.clone()))?;

        let activity_duration = prometheus::HistogramVec::new(
            prometheus::histogram_opts!(
                "saga_activity_duration_seconds",
                "Activity execution duration in seconds",
                activity_buckets
            ),
            &["activity_type"],
        )?;
        registry.register(Box::new(activity_duration.clone()))?;

        // Replay metrics
        let replay_duration = prometheus::HistogramVec::new(
            prometheus::histogram_opts!(
                "saga_replay_duration_seconds",
                "Workflow replay duration in seconds",
                vec![0.01, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0]
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(replay_duration.clone()))?;

        let replay_events = prometheus::HistogramVec::new(
            prometheus::histogram_opts!(
                "saga_replay_events_total",
                "Number of events replayed per workflow",
                vec![1.0, 10.0, 50.0, 100.0, 500.0, 1000.0]
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(replay_events.clone()))?;

        // Snapshot metrics
        let snapshot_taken = prometheus::IntCounterVec::new(
            prometheus::opts!("saga_snapshot_taken_total", "Total snapshots taken"),
            &["workflow_type"],
        )?;
        registry.register(Box::new(snapshot_taken.clone()))?;

        let snapshot_size = prometheus::HistogramVec::new(
            prometheus::histogram_opts!(
                "saga_snapshot_size_bytes",
                "Snapshot size in bytes",
                vec![1024.0, 10240.0, 102400.0, 1048576.0, 10485760.0]
            ),
            &["workflow_type"],
        )?;
        registry.register(Box::new(snapshot_size.clone()))?;

        Ok(Self {
            registry,
            workflow_started,
            workflow_completed,
            workflow_failed,
            workflow_duration,
            workflow_active,
            activity_started,
            activity_completed,
            activity_failed,
            activity_duration,
            replay_duration,
            replay_events,
            snapshot_taken,
            snapshot_size,
        })
    }

    /// Get the registry for custom metric registration
    pub fn registry(&self) -> &prometheus::Registry {
        &self.registry
    }

    /// Generate metrics output for HTTP response
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use saga_engine_metrics::PrometheusMetrics;
    ///
    /// let metrics = PrometheusMetrics::new()?;
    ///
    /// // In your HTTP handler
    /// let response = warp::http::Response::builder()
    ///     .header("Content-Type", metrics.content_type())
    ///     .body(metrics.gather())?;
    /// ```
    pub fn content_type(&self) -> &'static str {
        "text/plain; version=0.0.4; charset=utf-8"
    }

    /// Gather all metrics into a byte vector
    pub fn gather(&self) -> Vec<u8> {
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        let metric_families = self.registry.gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        buffer
    }

    /// Get HTTP handler for metrics endpoint
    ///
    /// Returns a closure that can be used with any HTTP framework
    ///
    /// # Example with warp
    ///
    /// ```rust,ignore
    /// use saga_engine_metrics::PrometheusMetrics;
    ///
    /// let metrics = PrometheusMetrics::new()?;
    /// let metrics = std::sync::Arc::new(metrics);
    ///
    /// let routes = warp::path("metrics")
    ///     .and(warp::get())
    ///     .map(move || {
    ///         let response = metrics.handler();
    ///         warp::http::Response::builder()
    ///             .header("Content-Type", metrics.content_type())
    ///             .body(response)
    ///             .unwrap()
    ///     });
    /// ```
    ///
    /// # Example with axum
    ///
    /// ```rust,ignore
    /// use saga_engine_metrics::PrometheusMetrics;
    ///
    /// let metrics = PrometheusMetrics::new()?;
    /// let metrics = std::sync::Arc::new(metrics);
    ///
    /// let app = axum::Router::new()
    ///     .route("/metrics", axum::routing::get(move || {
    ///         let body = metrics.gather();
    ///         axum::response::IntoResponse::into_response(
    ///             (axum::http::StatusCode::OK, body)
    ///         )
    ///     }));
    /// ```
    pub fn handler(&self) -> Vec<u8> {
        self.gather()
    }
}

impl SagaMetrics for PrometheusMetrics {
    fn workflow_started(&self, workflow_type: &str) {
        self.workflow_started
            .with_label_values(&[workflow_type])
            .inc();
        self.workflow_active
            .with_label_values(&[workflow_type])
            .inc();
    }

    fn workflow_completed(&self, workflow_type: &str, duration_ms: u64) {
        self.workflow_completed
            .with_label_values(&[workflow_type])
            .inc();
        self.workflow_duration
            .with_label_values(&[workflow_type])
            .observe(duration_ms as f64 / 1000.0);
        self.workflow_active
            .with_label_values(&[workflow_type])
            .dec();
    }

    fn workflow_failed(&self, workflow_type: &str, error_type: &str) {
        self.workflow_failed
            .with_label_values(&[workflow_type, error_type])
            .inc();
        self.workflow_active
            .with_label_values(&[workflow_type])
            .dec();
    }

    fn activity_started(&self, activity_type: &str) {
        self.activity_started
            .with_label_values(&[activity_type])
            .inc();
    }

    fn activity_completed(&self, activity_type: &str, duration_ms: u64) {
        self.activity_completed
            .with_label_values(&[activity_type])
            .inc();
        self.activity_duration
            .with_label_values(&[activity_type])
            .observe(duration_ms as f64 / 1000.0);
    }

    fn activity_failed(&self, activity_type: &str, error_type: &str) {
        self.activity_failed
            .with_label_values(&[activity_type, error_type])
            .inc();
    }

    fn replay_completed(&self, workflow_type: &str, duration_ms: u64, events_count: u64) {
        self.replay_duration
            .with_label_values(&[workflow_type])
            .observe(duration_ms as f64 / 1000.0);
        self.replay_events
            .with_label_values(&[workflow_type])
            .observe(events_count as f64);
    }

    fn snapshot_taken(&self, workflow_type: &str, size_bytes: u64) {
        self.snapshot_taken
            .with_label_values(&[workflow_type])
            .inc();
        self.snapshot_size
            .with_label_values(&[workflow_type])
            .observe(size_bytes as f64);
    }
}

/// Trait for saga metrics collection
pub trait SagaMetrics: Send + Sync + 'static {
    /// Called when a workflow starts
    fn workflow_started(&self, workflow_type: &str);

    /// Called when a workflow completes successfully
    fn workflow_completed(&self, workflow_type: &str, duration_ms: u64);

    /// Called when a workflow fails
    fn workflow_failed(&self, workflow_type: &str, error_type: &str);

    /// Called when an activity starts
    fn activity_started(&self, activity_type: &str);

    /// Called when an activity completes successfully
    fn activity_completed(&self, activity_type: &str, duration_ms: u64);

    /// Called when an activity fails
    fn activity_failed(&self, activity_type: &str, error_type: &str);

    /// Called when workflow replay completes
    fn replay_completed(&self, workflow_type: &str, duration_ms: u64, events_count: u64);

    /// Called when a snapshot is taken
    fn snapshot_taken(&self, workflow_type: &str, size_bytes: u64);
}

/// No-op metrics for testing
#[derive(Debug, Default, Clone)]
pub struct NoopSagaMetrics;

impl SagaMetrics for NoopSagaMetrics {
    fn workflow_started(&self, _workflow_type: &str) {}
    fn workflow_completed(&self, _workflow_type: &str, _duration_ms: u64) {}
    fn workflow_failed(&self, _workflow_type: &str, _error_type: &str) {}
    fn activity_started(&self, _activity_type: &str) {}
    fn activity_completed(&self, _activity_type: &str, _duration_ms: u64) {}
    fn activity_failed(&self, _activity_type: &str, _error_type: &str) {}
    fn replay_completed(&self, _workflow_type: &str, _duration_ms: u64, _events_count: u64) {}
    fn snapshot_taken(&self, _workflow_type: &str, _size_bytes: u64) {}
}

/// Wrapper for optional metrics that gracefully handles None
#[derive(Clone, Default)]
pub struct OptionalMetrics(pub Option<Arc<dyn SagaMetrics>>);

impl OptionalMetrics {
    /// Create new optional metrics wrapper
    pub fn new(metrics: Option<Arc<dyn SagaMetrics>>) -> Self {
        Self(metrics)
    }

    /// Check if metrics are enabled
    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }

    /// Called when a workflow starts
    pub fn workflow_started(&self, workflow_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.workflow_started(workflow_type);
        }
    }

    /// Called when a workflow completes successfully
    pub fn workflow_completed(&self, workflow_type: &str, duration_ms: u64) {
        if let Some(metrics) = &self.0 {
            metrics.workflow_completed(workflow_type, duration_ms);
        }
    }

    /// Called when a workflow fails
    pub fn workflow_failed(&self, workflow_type: &str, error_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.workflow_failed(workflow_type, error_type);
        }
    }

    /// Called when an activity starts
    pub fn activity_started(&self, activity_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.activity_started(activity_type);
        }
    }

    /// Called when an activity completes successfully
    pub fn activity_completed(&self, activity_type: &str, duration_ms: u64) {
        if let Some(metrics) = &self.0 {
            metrics.activity_completed(activity_type, duration_ms);
        }
    }

    /// Called when an activity fails
    pub fn activity_failed(&self, activity_type: &str, error_type: &str) {
        if let Some(metrics) = &self.0 {
            metrics.activity_failed(activity_type, error_type);
        }
    }

    /// Called when workflow replay completes
    pub fn replay_completed(&self, workflow_type: &str, duration_ms: u64, events_count: u64) {
        if let Some(metrics) = &self.0 {
            metrics.replay_completed(workflow_type, duration_ms, events_count);
        }
    }

    /// Called when a snapshot is taken
    pub fn snapshot_taken(&self, workflow_type: &str, size_bytes: u64) {
        if let Some(metrics) = &self.0 {
            metrics.snapshot_taken(workflow_type, size_bytes);
        }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Whether metrics collection is enabled
    pub enabled: bool,
    /// HTTP endpoint path for metrics
    pub path: String,
    /// Histogram buckets for workflow duration
    pub workflow_duration_buckets: Vec<f64>,
    /// Histogram buckets for activity duration
    pub activity_duration_buckets: Vec<f64>,
    /// Histogram buckets for replay duration
    pub replay_duration_buckets: Vec<f64>,
    /// Histogram buckets for snapshot size
    pub snapshot_size_buckets: Vec<f64>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/metrics".to_string(),
            workflow_duration_buckets: vec![0.1, 0.5, 1.0, 5.0, 30.0, 60.0, 300.0, 1800.0],
            activity_duration_buckets: vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 30.0],
            replay_duration_buckets: vec![0.01, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0],
            snapshot_size_buckets: vec![1024.0, 10240.0, 102400.0, 1048576.0, 10485760.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_metrics_creation() {
        let metrics = PrometheusMetrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_prometheus_metrics_with_custom_buckets() {
        let metrics = PrometheusMetrics::with_buckets(vec![0.1, 1.0, 10.0], vec![0.01, 0.1, 1.0]);
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_noop_metrics() {
        let metrics = NoopSagaMetrics::default();
        metrics.workflow_started("test");
        metrics.workflow_completed("test", 100);
        metrics.workflow_failed("test", "error");
        metrics.activity_started("test");
        metrics.activity_completed("test", 50);
        metrics.activity_failed("test", "error");
        metrics.replay_completed("test", 25, 10);
        metrics.snapshot_taken("test", 1024);
    }

    #[test]
    fn test_optional_metrics() {
        let noop: Arc<dyn SagaMetrics> = Arc::new(NoopSagaMetrics::default());
        let metrics = OptionalMetrics::new(Some(noop));
        metrics.workflow_started("test");
        metrics.activity_completed("test", 100);

        let empty = OptionalMetrics::new(None);
        assert!(!empty.is_enabled());
        empty.workflow_started("test");
    }

    #[test]
    fn test_handler_returns_valid_response() {
        let metrics = PrometheusMetrics::new().unwrap();

        // Trigger some metrics to ensure they appear in output
        metrics.workflow_started("test_workflow");
        metrics.workflow_completed("test_workflow", 100);
        metrics.activity_started("test_activity");
        metrics.activity_completed("test_activity", 50);

        let response = metrics.handler();

        // Response should contain Prometheus format
        let text = String::from_utf8(response).unwrap();
        assert!(text.contains("# HELP saga_workflow_started_total"));
        assert!(text.contains("# TYPE saga_workflow_started_total counter"));
        assert!(text.contains("saga_workflow_started_total{workflow_type=\"test_workflow\"} 1"));
    }

    #[test]
    fn test_content_type() {
        let metrics = PrometheusMetrics::new().unwrap();
        assert_eq!(
            metrics.content_type(),
            "text/plain; version=0.0.4; charset=utf-8"
        );
    }
}
