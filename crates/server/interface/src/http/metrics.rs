//! HTTP handlers for Prometheus metrics exposition

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;

/// Metrics state for the HTTP server
#[derive(Clone)]
pub struct MetricsState {
    pub registry: Arc<prometheus::Registry>,
}

/// Create the metrics router
pub fn metrics_router(registry: Arc<prometheus::Registry>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(MetricsState { registry })
}

/// Handler for Prometheus metrics exposition
async fn metrics_handler(
    State(state): State<MetricsState>,
) -> Result<Response, (axum::http::StatusCode, String)> {
    let encoder = TextEncoder::new();
    let metric_families = state.registry.gather();

    let mut buffer = Vec::new();
    encoder
        .encode(&metric_families, &mut buffer)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let response = String::from_utf8(buffer)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        response,
    )
        .into_response())
}

/// Metrics event subscriber that automatically records metrics from events
pub struct MetricsEventSubscriber {
    registry: Arc<prometheus::Registry>,
}

impl MetricsEventSubscriber {
    /// Create a new metrics event subscriber
    pub fn new(registry: Arc<prometheus::Registry>) -> Self {
        Self { registry }
    }

    /// Handle a domain event and update metrics accordingly
    pub async fn handle_event(
        &self,
        event: &hodei_server_domain::events::DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use hodei_server_domain::events::DomainEvent;

        match event {
            // Business metrics - Job lifecycle
            DomainEvent::JobCreated { job_id, spec, .. } => {
                let job_type = "shell"; // Default, could be extracted from spec
                record_business_metric!(jobs_created, job_type, "default", "us-east-1");
            }
            DomainEvent::JobStatusChanged { new_state, .. } => {
                let status = match new_state {
                    hodei_server_domain::shared_kernel::JobState::Succeeded => "success",
                    hodei_server_domain::shared_kernel::JobState::Failed => "failure",
                    hodei_server_domain::shared_kernel::JobState::Running => "running",
                    _ => "other",
                };
                record_business_metric!(jobs_completed, "shell", "default", "us-east-1", status);
            }
            DomainEvent::JobAssigned { .. } => {
                // Decrement queue depth when job is assigned
                record_system_metric!(events_processed, "JobAssigned");
            }
            DomainEvent::WorkerRegistered { worker_id, .. } => {
                record_business_metric!(worker_count, "docker", "ready");
            }
            DomainEvent::WorkerDisconnected { .. } => {
                record_business_metric!(worker_count, "docker", "disconnected");
            }
            DomainEvent::WorkerProvisioned { worker_id, .. } => {
                record_system_metric!(events_processed, "WorkerProvisioned");
            }
            _ => {
                // Record all events as processed
                record_system_metric!(events_processed, event.event_type());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let registry = Arc::new(prometheus::Registry::new());
        let app = metrics_router(registry.clone());
        let server = TestServer::new(app).unwrap();

        let response = server.get("/metrics").await;
        assert!(response.ok());
        assert!(response.text().contains("# HELP"));
        assert!(response.text().contains("hodei_"));
    }

    #[tokio::test]
    async fn test_metrics_content_type() {
        let registry = Arc::new(prometheus::Registry::new());
        let app = metrics_router(registry.clone());
        let server = TestServer::new(app).unwrap();

        let response = server.get("/metrics").await;
        assert_eq!(
            response.headers()["Content-Type"],
            "text/plain; version=0.0.4; charset=utf-8"
        );
    }
}
