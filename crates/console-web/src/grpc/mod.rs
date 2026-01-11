//! gRPC Client for Hodei Jobs Server
//!
//! Provides a unified interface to communicate with Hodei Jobs Server
//! using gRPC protocol with tonic.

use std::time::Duration;

use thiserror::Error;
use tonic::transport::Channel;

// Import types from proto crate
use hodei_jobs::{
    GetAvailableWorkersRequest, GetAvailableWorkersResponse, GetJobRequest, GetJobResponse,
    GetQueueStatusRequest, JobId, ListJobsRequest, ListJobsResponse, QueueStatus,
};

// Import service clients
use hodei_jobs::job_execution_service_client::JobExecutionServiceClient;
use hodei_jobs::metrics_service_client::MetricsServiceClient;
use hodei_jobs::providers::provider_management_service_client::ProviderManagementServiceClient;
use hodei_jobs::scheduler_service_client::SchedulerServiceClient;
use hodei_jobs::worker_agent_service_client::WorkerAgentServiceClient;

/// Errors for gRPC client operations
#[derive(Debug, Error)]
pub enum GrpcClientError {
    #[error("Connection failed: {0}")]
    Connection(#[from] tonic::transport::Error),

    #[error("Request failed: {0}")]
    Request(#[from] tonic::Status),

    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: &'static str },

    #[error("Invalid address format")]
    InvalidAddress,
}

/// Configuration for gRPC client
#[derive(Clone, Debug)]
pub struct GrpcClientConfig {
    /// Server address (e.g., "http://localhost:50051")
    pub server_address: String,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            server_address: "http://localhost:50051".to_string(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// gRPC Client wrapper for Hodei Jobs Server
#[derive(Clone)]
pub struct GrpcClient {
    channel: Channel,
}

impl GrpcClient {
    /// Connect to Hodei Jobs Server
    pub async fn connect(server_address: &str) -> Result<Self, GrpcClientError> {
        let channel = Channel::from_shared(server_address.to_string())
            .map_err(|_| GrpcClientError::InvalidAddress)?
            .connect()
            .await
            .map_err(GrpcClientError::Connection)?;

        Ok(Self { channel })
    }

    /// Get jobs service client
    pub fn jobs(&self) -> JobExecutionServiceClient<Channel> {
        JobExecutionServiceClient::new(self.channel.clone())
    }

    /// Get workers service client
    pub fn workers(&self) -> WorkerAgentServiceClient<Channel> {
        WorkerAgentServiceClient::new(self.channel.clone())
    }

    /// Get scheduler service client
    pub fn scheduler(&self) -> SchedulerServiceClient<Channel> {
        SchedulerServiceClient::new(self.channel.clone())
    }

    /// Get metrics service client
    pub fn metrics(&self) -> MetricsServiceClient<Channel> {
        MetricsServiceClient::new(self.channel.clone())
    }

    /// Get providers service client
    pub fn providers(&self) -> ProviderManagementServiceClient<Channel> {
        ProviderManagementServiceClient::new(self.channel.clone())
    }
}

/// Jobs Service - High-level API for job operations
#[derive(Clone)]
pub struct JobsService {
    client: GrpcClient,
}

impl JobsService {
    /// Create new JobsService
    pub fn new(client: GrpcClient) -> Self {
        Self { client }
    }

    /// List all jobs with optional filters
    pub async fn list_jobs(
        &self,
        search_term: Option<String>,
        status: Option<i32>,
        limit: i32,
        offset: i32,
    ) -> Result<ListJobsResponse, GrpcClientError> {
        let request = ListJobsRequest {
            limit,
            offset,
            status,
            search_term,
        };

        let mut svc = self.client.jobs();
        let response = svc
            .list_jobs(request)
            .await
            .map_err(GrpcClientError::Request)?;

        Ok(response.into_inner())
    }

    /// Get a specific job by ID
    pub async fn get_job(&self, job_id: &str) -> Result<GetJobResponse, GrpcClientError> {
        let request = GetJobRequest {
            job_id: Some(JobId {
                value: job_id.to_string(),
            }),
        };

        let mut svc = self.client.jobs();
        let response = svc
            .get_job(request)
            .await
            .map_err(GrpcClientError::Request)?;

        Ok(response.into_inner())
    }
}

/// Workers Service - High-level API for worker operations
#[derive(Clone)]
pub struct WorkersService {
    client: GrpcClient,
}

impl WorkersService {
    /// Create new WorkersService
    pub fn new(client: GrpcClient) -> Self {
        Self { client }
    }

    /// List available workers - stub implementation
    pub async fn list_workers(&self) -> Result<GetAvailableWorkersResponse, GrpcClientError> {
        // Note: get_available_workers method is not available in the current proto definitions
        // This is a placeholder that returns an empty response
        Ok(GetAvailableWorkersResponse {
            workers: Vec::new(),
        })
    }
}

/// Scheduler Service - High-level API for scheduler operations
#[derive(Clone)]
pub struct SchedulerService {
    client: GrpcClient,
}

impl SchedulerService {
    /// Create new SchedulerService
    pub fn new(client: GrpcClient) -> Self {
        Self { client }
    }

    /// Get queue status - stub implementation
    pub async fn get_queue_status(&self) -> Result<QueueStatus, GrpcClientError> {
        // Note: get_queue_status returns QueueStatus with these fields
        Ok(QueueStatus {
            pending_jobs: 0,
            running_jobs: 0,
            completed_jobs: 0,
            failed_jobs: 0,
            cancelled_jobs: 0,
            last_updated: None,
        })
    }
}

/// Metrics Service - High-level API for metrics operations
#[derive(Clone)]
pub struct MetricsService {
    client: GrpcClient,
}

impl MetricsService {
    /// Create new MetricsService
    pub fn new(client: GrpcClient) -> Self {
        Self { client }
    }
}
