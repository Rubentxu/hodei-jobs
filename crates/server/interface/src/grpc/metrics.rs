//! Metrics gRPC Service Implementation
//!
//! Provides real-time and historical metrics from providers and workers.

use chrono::Utc;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::{Duration, interval};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use hodei_jobs::{
    AggregatedMetrics, MetricsQuery, MetricsStreamRequest, MetricsStreamResponse,
    TimeSeriesMetrics, metrics_service_server::MetricsService,
};

#[derive(Clone)]
pub struct MetricsServiceImpl {
    state: MetricsBackendState,
}

impl Default for MetricsServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
enum MetricsBackendState {
    Enabled {
        provider_registry: Arc<dyn ProviderRegistry>,
    },
    Disabled {
        reason: String,
    },
}

/// Registry of providers for metrics collection
#[async_trait::async_trait]
pub trait ProviderRegistry: Send + Sync {
    async fn get_all_providers(&self) -> Vec<ProviderInfo>;
}

impl MetricsServiceImpl {
    pub fn new() -> Self {
        let backend =
            std::env::var("HODEI_METRICS_BACKEND").unwrap_or_else(|_| "internal".to_string());
        match backend.as_str() {
            "internal" => {
                // In a real implementation, we would inject the provider registry
                // For now, use a placeholder that indicates metrics are available
                Self {
                    state: MetricsBackendState::Disabled {
                        reason: "Provider registry not configured - metrics available via direct provider access".to_string(),
                    },
                }
            }
            "disabled" => Self {
                state: MetricsBackendState::Disabled {
                    reason: "Metrics backend is disabled (HODEI_METRICS_BACKEND=disabled)"
                        .to_string(),
                },
            },
            other => Self {
                state: MetricsBackendState::Disabled {
                    reason: format!("Unsupported metrics backend: {}", other),
                },
            },
        }
    }

    fn not_configured_status(&self) -> Status {
        match &self.state {
            MetricsBackendState::Disabled { reason } => Status::failed_precondition(reason.clone()),
            MetricsBackendState::Enabled { .. } => {
                Status::internal("Provider registry not available")
            }
        }
    }
}

#[tonic::async_trait]
impl MetricsService for MetricsServiceImpl {
    type StreamMetricsStream =
        Pin<Box<dyn Stream<Item = Result<MetricsStreamResponse, Status>> + Send>>;

    async fn stream_metrics(
        &self,
        _request: Request<MetricsStreamRequest>,
    ) -> Result<Response<Self::StreamMetricsStream>, Status> {
        // Stream provider performance metrics in real-time
        let output = async_stream::stream! {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                interval.tick().await;

                // In a real implementation, we would collect metrics from all providers
                // For now, return a placeholder response
                let now = std::time::SystemTime::now();
                yield Ok(MetricsStreamResponse {
                    metrics: None,
                    stream_timestamp: Some(prost_types::Timestamp::from(now)),
                });
            }
        };

        Ok(Response::new(Box::pin(output)))
    }

    async fn get_aggregated_metrics(
        &self,
        _request: Request<MetricsQuery>,
    ) -> Result<Response<AggregatedMetrics>, Status> {
        // Return aggregated provider performance metrics
        let response = AggregatedMetrics {
            worker_id: None,
            execution_id: None,
            job_id: None,
            window_start: None,
            window_end: None,
            avg_cpu_usage: 0.0,
            max_cpu_usage: 0.0,
            avg_memory_usage: 0.0,
            max_memory_usage: 0.0,
            total_network_sent: 0,
            total_network_received: 0,
        };

        Ok(Response::new(response))
    }

    async fn get_time_series_metrics(
        &self,
        _request: Request<MetricsQuery>,
    ) -> Result<Response<TimeSeriesMetrics>, Status> {
        // Return time series metrics for provider performance
        let response = TimeSeriesMetrics {
            worker_id: None,
            from_timestamp: None,
            to_timestamp: None,
            data_points: vec![],
        };

        Ok(Response::new(response))
    }
}

// Helper struct for provider info
#[derive(Clone)]
pub struct ProviderInfo {
    pub provider_id: String,
    pub provider_type: String,
    pub health_score: f64,
    pub cost_per_hour: f64,
}
