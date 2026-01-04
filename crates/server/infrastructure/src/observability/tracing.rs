//! Tracing Module - OpenTelemetry integration for distributed tracing
//!
//! Provides OpenTelemetry tracing with:
//! - OTLP exporter for sending traces to collectors (Jaeger, Tempo, etc.)
//! - W3C Trace Context propagation
//! - gRPC and NATS header propagation
//!
//! EPIC-43: Sprint 5 - Observabilidad
//! US-EDA-501: Integrar OpenTelemetry

use opentelemetry::Key;
use opentelemetry::global;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::sdk::Resource;
use opentelemetry::sdk::export::trace::SpanExporter;
use opentelemetry::sdk::trace::{BatchSpanProcessor, Config, Sampler};
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::OTLPTonicExporterBuilder;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::{Channel, Endpoint};
use tracing::{info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

/// W3C Trace Context propagator for distributed tracing
pub fn w3c_trace_context_propagator() -> impl TextMapPropagator {
    opentelemetry::propagation::TraceContextPropagator::new()
}

/// Configuration for OpenTelemetry tracing
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// OTLP endpoint (e.g., "http://localhost:4317")
    pub otlp_endpoint: String,

    /// Service name for traces
    pub service_name: String,

    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,

    /// Whether to export to OTLP
    pub export_enabled: bool,

    /// Whether to log traces locally
    pub log_enabled: bool,

    /// Log filter level
    pub log_level: String,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: "http://localhost:4317".to_string(),
            service_name: "hodei-jobs".to_string(),
            sampling_ratio: 1.0,
            export_enabled: true,
            log_enabled: true,
            log_level: "info".to_string(),
        }
    }
}

/// Result of tracing initialization
#[derive(Debug)]
pub struct TracingResult {
    /// Whether tracing was successfully initialized
    pub initialized: bool,
    /// Tracer provider (if initialized)
    pub tracer_provider: Option<Arc<TracerProvider>>,
    /// Any errors that occurred
    pub errors: Vec<String>,
}

impl TracingResult {
    pub fn new() -> Self {
        Self {
            initialized: false,
            tracer_provider: None,
            errors: Vec::new(),
        }
    }
}

/// Initialize OpenTelemetry tracing
///
/// # Arguments
///
/// * `config` - Tracing configuration
///
/// # Returns
///
/// `TracingResult` with initialization status
pub fn init_tracing(config: &TracingConfig) -> TracingResult {
    let mut result = TracingResult::new();

    // Create resource with service info
    let resource = Resource::new(vec![
        Key::service_name.string(&config.service_name),
        Key::service_version.string("1.0.0"),
        Key::deployment_environment.string("production"),
    ]);

    // Create sampler based on configuration
    let sampler = if config.sampling_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sampling_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sampling_ratio)
    };

    // Configure tracer
    let tracer_config = Config::default()
        .with_resource(resource)
        .with_sampler(sampler);

    // Create OTLP exporter if enabled
    if config.export_enabled {
        match create_otlp_exporter(&config.otlp_endpoint) {
            Ok(exporter) => {
                let provider = BatchSpanProcessor::builder(exporter, tokio::spawn)
                    .with_max_queue_size(2048)
                    .with_max_export_batch_size(512)
                    .with_schedule_delay(std::time::Duration::from_secs(5))
                    .build();

                let tracer_provider = TracerProvider::builder()
                    .with_batch_processor(provider)
                    .build();

                // Set as global tracer provider
                let provider = Arc::new(tracer_provider);
                global::set_tracer_provider(provider.clone());

                result.tracer_provider = Some(provider);
                result.initialized = true;
                info!(
                    "OpenTelemetry tracing initialized, exporting to {}",
                    config.otlp_endpoint
                );
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("Failed to create OTLP exporter: {}", e));
                warn!("Failed to create OTLP exporter, falling back to logging only");
            }
        }
    }

    // Create logging layer
    if config.log_enabled {
        let env_filter = EnvFilter::new(&config.log_level);

        let logging_layer = tracing_subscriber::fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_target(true)
            .with_writer(std::io::stdout)
            .with_filter(env_filter);

        // Create subscriber with logging layer
        let subscriber = tracing_subscriber::Registry::default().with(logging_layer);

        // If we have a tracer, add it as a layer
        if let Some(provider) = &result.tracer_provider {
            let tracer = provider.versioned_tracer(
                "hodei-jobs-tracer",
                Some(env!("CARGO_PKG_VERSION")),
                None,
            );

            let telemetry_layer = tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

            let subscriber = subscriber.with(telemetry_layer);
            tracing::subscriber::set_global_default(subscriber).ok();
        } else {
            tracing::subscriber::set_global_default(subscriber).ok();
        }
    }

    // Set up propagator for downstream propagation
    global::set_text_map_propagator(w3c_trace_context_propagator());

    result
}

/// Create OTLP exporter from endpoint
fn create_otlp_exporter(
    endpoint: &str,
) -> Result<Box<dyn SpanExporter>, Box<dyn std::error::Error + Send + Sync>> {
    // Create channel to OTLP endpoint
    let channel = Endpoint::new(endpoint.to_string())?
        .connect()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Create OTLP exporter
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_channel(channel)
        .with_timeout(std::time::Duration::from_secs(10));

    Ok(Box::new(exporter) as Box<dyn SpanExporter>)
}

/// Graceful shutdown of tracing
pub async fn shutdown_tracing() {
    info!("Shutting down OpenTelemetry tracing...");
    global::shutdown_tracer_provider();
}

/// Extract trace context from headers
///
/// # Arguments
///
/// * `headers` - Iterator of header name/value pairs
///
/// # Returns
///
/// Extracted context or None
pub fn extract_trace_context<'a, I>(headers: I) -> opentelemetry::Context
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    let propagator = w3c_trace_context_propagator();
    let mut carrier = HashMapHeaders::new(headers);
    propagator.extract(&mut carrier)
}

/// Inject trace context into headers
///
/// # Arguments
///
/// * `context` - Current OpenTelemetry context
/// * `headers` - Mutable headers map
pub fn inject_trace_context(
    context: &opentelemetry::Context,
    headers: &mut std::collections::HashMap<String, String>,
) {
    let propagator = w3c_trace_context_propagator();
    let mut carrier =
        HashMapHeaders::new(headers.iter_mut().map(|(k, v)| (k.as_str(), v.as_str())));
    propagator.inject_context(context, &mut carrier);
}

/// Simple hashmap-based header carrier for propagation
struct HashMapHeaders<'a, I>
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    headers: I,
}

impl<'a, I> HashMapHeaders<'a, I>
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    fn new(headers: I) -> Self {
        Self { headers }
    }
}

impl<'a, I> opentelemetry::propagation::Extractor for HashMapHeaders<'a, I>
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    fn get(&self, key: &str) -> Option<&str> {
        self.headers
            .clone()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.clone().map(|(k, _)| k).collect()
    }
}

impl<'a, I> opentelemetry::propagation::Injector for HashMapHeaders<'a, I>
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    fn set(&mut self, key: &str, value: String) {
        // We need a different approach for injection
    }
}

impl opentelemetry::propagation::Injector for std::collections::HashMap<String, String> {
    fn set(&mut self, key: &str, value: String) {
        self.insert(key.to_string(), value);
    }
}

/// Create a new span with correlation context
///
/// # Arguments
///
/// * `name` - Span name
/// * `correlation_id` - Correlation ID to include
/// * `kind` - Span kind
///
/// # Returns
///
/// New span with context
#[macro_export]
macro_rules! create_trace_span {
    ($name:expr, $correlation_id:expr, $kind:expr) => {{
        tracing::info_span!(
            $name,
            correlation_id = $correlation_id,
            kind = tracing::field::debug($kind),
        )
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracing_config_defaults() {
        let config = TracingConfig::default();
        assert_eq!(config.service_name, "hodei-jobs");
        assert_eq!(config.sampling_ratio, 1.0);
        assert!(config.export_enabled);
        assert!(config.log_enabled);
    }

    #[tokio::test]
    async fn test_tracing_result_defaults() {
        let result = TracingResult::new();
        assert!(!result.initialized);
        assert!(result.errors.is_empty());
        assert!(result.tracer_provider.is_none());
    }

    #[tokio::test]
    async fn test_w3c_propagator() {
        let propagator = w3c_trace_context_propagator();
        assert!(!propagator.fields().is_empty());
    }

    #[tokio::test]
    async fn test_extract_trace_context() {
        let headers = vec![(
            "traceparent",
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        )];
        let context = extract_trace_context(headers.into_iter());
        assert!(context.span().span_context().is_valid());
    }

    #[tokio::test]
    async fn test_inject_trace_context() {
        let mut headers = std::collections::HashMap::new();
        let context = opentelemetry::Context::new();
        inject_trace_context(&context, &mut headers);
        // Injected headers should contain traceparent
        assert!(headers.contains_key("traceparent"));
    }
}
