//! Metrics gRPC Service Implementation

use std::pin::Pin;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use hodei_jobs::{
    metrics_service_server::MetricsService,
    MetricsStreamRequest, MetricsStreamResponse,
    MetricsQuery, AggregatedMetrics, TimeSeriesMetrics,
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
    Disabled { reason: String },
}

impl MetricsServiceImpl {
    pub fn new() -> Self {
        let backend = std::env::var("HODEI_METRICS_BACKEND").unwrap_or_else(|_| "disabled".to_string());
        match backend.as_str() {
            "disabled" => Self {
                state: MetricsBackendState::Disabled {
                    reason: "Metrics backend is disabled (no backend configured)".to_string(),
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
        }
    }
}

#[tonic::async_trait]
impl MetricsService for MetricsServiceImpl {
    type StreamMetricsStream = Pin<Box<dyn Stream<Item = Result<MetricsStreamResponse, Status>> + Send>>;

    async fn stream_metrics(
        &self,
        _request: Request<MetricsStreamRequest>,
    ) -> Result<Response<Self::StreamMetricsStream>, Status> {
        Err(self.not_configured_status())
    }

    async fn get_aggregated_metrics(
        &self,
        _request: Request<MetricsQuery>,
    ) -> Result<Response<AggregatedMetrics>, Status> {
        Err(self.not_configured_status())
    }

    async fn get_time_series_metrics(
        &self,
        _request: Request<MetricsQuery>,
    ) -> Result<Response<TimeSeriesMetrics>, Status> {
        Err(self.not_configured_status())
    }
}
