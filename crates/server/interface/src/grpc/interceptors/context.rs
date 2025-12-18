//! Context Interceptor
//!
//! Interceptor gRPC para extraer o generar correlation_id y actor
//! de los headers de metadata y propagarlos a través de las capas.

use hodei_server_domain::request_context::RequestContext;
use tonic::{Request, Status};
use uuid::Uuid;

/// Header name para correlation ID
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";
/// Header name para actor/user ID
pub const ACTOR_HEADER: &str = "x-actor-id";
/// Header name para trace ID (compatible con OpenTelemetry)
pub const TRACE_ID_HEADER: &str = "x-trace-id";

/// Extrae o genera un RequestContext desde los metadata de una request gRPC
pub fn extract_request_context<T>(request: &Request<T>) -> RequestContext {
    let metadata = request.metadata();

    let correlation_id = metadata
        .get(CORRELATION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let actor = metadata
        .get(ACTOR_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    match actor {
        Some(actor_value) => RequestContext::with_actor(correlation_id, actor_value),
        None => RequestContext::with_correlation_id(correlation_id),
    }
}

/// Interceptor function para usar con tonic
///
/// Uso:
/// ```ignore
/// let service = MyServiceServer::with_interceptor(service, context_interceptor);
/// ```
pub fn context_interceptor(mut request: Request<()>) -> Result<Request<()>, Status> {
    let ctx = extract_request_context(&request);
    request.extensions_mut().insert(ctx);
    Ok(request)
}

/// Extension trait para extraer RequestContext de una Request
pub trait RequestContextExt {
    /// Extrae el RequestContext de los extensions o lo genera desde metadata
    fn get_context(&self) -> RequestContext;
}

impl<T> RequestContextExt for Request<T> {
    fn get_context(&self) -> RequestContext {
        self.extensions()
            .get::<RequestContext>()
            .cloned()
            .unwrap_or_else(|| extract_request_context(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    #[test]
    fn test_extract_context_with_all_headers() {
        let mut request = Request::new(());
        request.metadata_mut().insert(
            CORRELATION_ID_HEADER,
            MetadataValue::from_static("test-correlation-123"),
        );
        request
            .metadata_mut()
            .insert(ACTOR_HEADER, MetadataValue::from_static("user@example.com"));

        let ctx = extract_request_context(&request);

        assert_eq!(ctx.correlation_id(), "test-correlation-123");
        assert_eq!(ctx.get_actor(), Some("user@example.com"));
    }

    #[test]
    fn test_extract_context_generates_correlation_id_when_missing() {
        let request = Request::new(());
        let ctx = extract_request_context(&request);

        assert!(!ctx.correlation_id().is_empty());
        assert!(Uuid::parse_str(ctx.correlation_id()).is_ok());
        assert!(ctx.get_actor().is_none());
    }

    #[test]
    fn test_extract_context_with_only_correlation_id() {
        let mut request = Request::new(());
        request.metadata_mut().insert(
            CORRELATION_ID_HEADER,
            MetadataValue::from_static("only-corr-id"),
        );

        let ctx = extract_request_context(&request);

        assert_eq!(ctx.correlation_id(), "only-corr-id");
        assert!(ctx.get_actor().is_none());
    }

    #[test]
    fn test_context_interceptor_inserts_context() {
        let mut request = Request::new(());
        request.metadata_mut().insert(
            CORRELATION_ID_HEADER,
            MetadataValue::from_static("interceptor-test"),
        );

        let result = context_interceptor(request);
        assert!(result.is_ok());

        let request = result.unwrap();
        let ctx = request.extensions().get::<RequestContext>();
        assert!(ctx.is_some());
        assert_eq!(ctx.unwrap().correlation_id(), "interceptor-test");
    }

    #[test]
    fn test_request_context_ext_trait() {
        let mut request = Request::new(());
        request.metadata_mut().insert(
            CORRELATION_ID_HEADER,
            MetadataValue::from_static("ext-trait-test"),
        );
        request
            .metadata_mut()
            .insert(ACTOR_HEADER, MetadataValue::from_static("admin"));

        let ctx = request.get_context();

        assert_eq!(ctx.correlation_id(), "ext-trait-test");
        assert_eq!(ctx.get_actor(), Some("admin"));
    }

    #[test]
    fn test_request_context_ext_uses_cached_context() {
        let mut request = Request::new(());

        let cached_ctx = RequestContext::with_actor("cached-id", "cached-actor");
        request.extensions_mut().insert(cached_ctx);

        request.metadata_mut().insert(
            CORRELATION_ID_HEADER,
            MetadataValue::from_static("should-be-ignored"),
        );

        let ctx = request.get_context();

        assert_eq!(ctx.correlation_id(), "cached-id");
        assert_eq!(ctx.get_actor(), Some("cached-actor"));
    }
}
