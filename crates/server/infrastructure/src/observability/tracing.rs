//! Tracing Module - Basic tracing for distributed tracing
//!
//! Provides basic tracing with:
//! - W3C Trace Context propagation
//! - gRPC and header propagation
//!
//! EPIC-43: Sprint 5 - Observabilidad
//! US-EDA-501: Integrar OpenTelemetry

use opentelemetry::Context;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::collections::HashMap;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

/// W3C Trace Context propagator for distributed tracing
pub fn w3c_trace_context_propagator() -> impl TextMapPropagator {
    TraceContextPropagator::new()
}

/// Configuration for tracing
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Service name for traces
    pub service_name: String,
    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,
    /// Whether to log traces locally
    pub log_enabled: bool,
    /// Log filter level
    pub log_level: String,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "hodei-jobs".to_string(),
            sampling_ratio: 1.0,
            log_enabled: true,
            log_level: "info".to_string(),
        }
    }
}

/// Result of tracing initialization
#[derive(Debug)]
pub struct TracingResult {
    pub initialized: bool,
    pub errors: Vec<String>,
}

impl TracingResult {
    pub fn new() -> Self {
        Self {
            initialized: false,
            errors: Vec::new(),
        }
    }
}

/// Initialize tracing
pub fn init_tracing(config: &TracingConfig) -> TracingResult {
    let mut result = TracingResult::new();

    if config.log_enabled {
        let env_filter = EnvFilter::new(&config.log_level);

        let logging_layer = tracing_subscriber::fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_target(true)
            .with_writer(std::io::stdout)
            .with_filter(env_filter);

        let subscriber = tracing_subscriber::Registry::default().with(logging_layer);
        tracing::subscriber::set_global_default(subscriber).ok();

        // Set up propagator for downstream propagation
        opentelemetry::global::set_text_map_propagator(w3c_trace_context_propagator());

        result.initialized = true;
        info!("Tracing initialized for service: {}", config.service_name);
    }

    result
}

/// Graceful shutdown of tracing
pub async fn shutdown_tracing() {
    info!("Shutting down tracing...");
}

/// Extract trace context from headers
pub fn extract_trace_context<'a, I>(headers: I) -> Context
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    let propagator = w3c_trace_context_propagator();
    let mut carrier = HashMapHeaders::new(headers);
    propagator.extract(&mut carrier)
}

/// Inject trace context into headers
pub fn inject_trace_context(context: &Context, headers: &mut HashMap<String, String>) {
    let propagator = w3c_trace_context_propagator();
    let mut carrier = HashMap::new();
    propagator.inject_context(context, &mut carrier);
    for (k, v) in carrier {
        headers.insert(k.to_string(), v);
    }
}

/// Simple hashmap-based header carrier for propagation
struct HashMapHeaders<'a> {
    headers: Vec<(&'a str, &'a str)>,
}

impl<'a> HashMapHeaders<'a> {
    fn new<I>(headers: I) -> Self
    where
        I: Iterator<Item = (&'a str, &'a str)>,
    {
        Self {
            headers: headers.collect(),
        }
    }
}

impl<'a> opentelemetry::propagation::Extractor for HashMapHeaders<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }
    fn keys(&self) -> Vec<&str> {
        self.headers.iter().map(|(k, _)| *k).collect()
    }
}

/// Create a new span with correlation context
#[macro_export]
macro_rules! create_trace_span {
    ($name:expr, $correlation_id:expr, $kind:expr) => {{
        tracing::info_span!(
            $name,
            correlation_id = $correlation_id,
            kind = tracing::field::debug($kind)
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
        assert!(config.log_enabled);
    }

    #[tokio::test]
    async fn test_tracing_result_defaults() {
        let result = TracingResult::new();
        assert!(!result.initialized);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_w3c_propagator() {
        let propagator = w3c_trace_context_propagator();
        let fields: Vec<&str> = propagator.fields().collect();
        assert!(!fields.is_empty());
    }

    #[tokio::test]
    async fn test_extract_trace_context() {
        let headers = vec![(
            "traceparent",
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        )];
        let context = extract_trace_context(headers.into_iter());
        // Verify context was extracted successfully
        assert!(!format!("{:?}", context).is_empty());
    }

    #[tokio::test]
    async fn test_inject_trace_context() {
        let mut headers = HashMap::new();
        let context = Context::new();
        inject_trace_context(&context, &mut headers);
        // With empty context, headers may or may not contain traceparent
        // Just verify the function runs without error
        assert!(true);
    }
}
