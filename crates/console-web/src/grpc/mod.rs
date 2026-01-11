//! gRPC Client for Hodei Jobs Server with Reconnection Support
//!
//! Provides a unified interface to communicate with Hodei Jobs Server
//! using gRPC protocol with tonic, including automatic reconnection.

use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::{interval, timeout};
use tonic::Status;
use tonic::transport::{Channel, Endpoint};
use tracing::{error, info, warn};

// Import types from proto crate
use hodei_jobs::{GetJobRequest, GetJobResponse, JobId, ListJobsRequest, ListJobsResponse};

// Import service clients
use hodei_jobs::job_execution_service_client::JobExecutionServiceClient;
use hodei_jobs::scheduler_service_client::SchedulerServiceClient;

/// Connection state
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Errors for gRPC client operations
#[derive(Debug, Error)]
pub enum GrpcClientError {
    #[error("Connection failed")]
    Connection,

    #[error("Request failed: {0}")]
    Request(#[from] Status),

    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: &'static str },

    #[error("Invalid address format")]
    InvalidAddress,

    #[error("Not connected to server")]
    NotConnected,

    #[error("Max reconnection attempts exceeded")]
    MaxRetriesExceeded,

    #[error("Connection timeout")]
    Timeout,
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
    /// Enable automatic reconnection
    pub enable_reconnect: bool,
    /// Reconnection interval
    pub reconnect_interval: Duration,
    /// Max reconnection attempts
    pub max_retries: u32,
    /// Scheduler name for requests
    pub scheduler_name: String,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            server_address: "http://localhost:50051".to_string(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            enable_reconnect: true,
            reconnect_interval: Duration::from_secs(2),
            max_retries: 5,
            scheduler_name: "default".to_string(),
        }
    }
}

/// gRPC Client wrapper with connection management
#[derive(Clone)]
pub struct GrpcClient {
    /// Shared state for connection management
    state: Arc<RwLock<ConnectionState>>,
    /// Server address
    address: String,
    /// Channel for gRPC requests
    channel: Arc<RwLock<Option<Channel>>>,
    /// Configuration
    config: GrpcClientConfig,
}

impl GrpcClient {
    /// Create a new gRPC client
    pub fn new(address: String, config: GrpcClientConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            address,
            channel: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Connect to the server
    pub async fn connect(&self) -> Result<(), GrpcClientError> {
        *self.state.write().await = ConnectionState::Connecting;

        match self.create_channel().await {
            Ok(channel) => {
                *self.channel.write().await = Some(channel);
                *self.state.write().await = ConnectionState::Connected;
                Ok(())
            }
            Err(e) => {
                *self.state.write().await = ConnectionState::Disconnected;
                Err(e)
            }
        }
    }

    /// Connect with automatic retry
    pub async fn connect_with_retry(&self) -> Result<(), GrpcClientError> {
        let mut attempts = 0u32;
        let max_retries = self.config.max_retries;

        while attempts < max_retries {
            match self.connect().await {
                Ok(()) => {
                    if attempts > 0 {
                        info!("Reconnected to gRPC server after {} attempts", attempts);
                    }
                    return Ok(());
                }
                Err(e) => {
                    attempts += 1;
                    warn!("Connection attempt {} failed: {}", attempts, e);

                    if attempts < max_retries {
                        *self.state.write().await = ConnectionState::Reconnecting;
                        tokio::time::sleep(self.config.reconnect_interval).await;
                    }
                }
            }
        }

        *self.state.write().await = ConnectionState::Disconnected;
        Err(GrpcClientError::MaxRetriesExceeded)
    }

    /// Create a new channel to the server
    async fn create_channel(&self) -> Result<Channel, GrpcClientError> {
        let endpoint = Endpoint::from_shared(self.address.clone())
            .map_err(|_| GrpcClientError::InvalidAddress)?
            .connect_timeout(self.config.connect_timeout);

        timeout(self.config.connect_timeout, endpoint.connect())
            .await
            .map_err(|_| GrpcClientError::Timeout)?
            .map_err(|_| GrpcClientError::Connection)
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == ConnectionState::Connected
    }

    /// Get connection state
    pub async fn state(&self) -> ConnectionState {
        self.state.read().await.clone()
    }

    /// Get a channel, reconnecting if necessary
    async fn get_channel(&self) -> Result<Channel, GrpcClientError> {
        if let Some(channel) = self.channel.read().await.clone() {
            return Ok(channel);
        }

        if self.config.enable_reconnect {
            self.connect_with_retry().await?;
            Ok(self.channel.read().await.clone().unwrap())
        } else {
            Err(GrpcClientError::NotConnected)
        }
    }

    /// Get jobs service client
    pub async fn jobs(&self) -> Result<JobExecutionServiceClient<Channel>, GrpcClientError> {
        let channel = self.get_channel().await?;
        Ok(JobExecutionServiceClient::new(channel))
    }

    /// Get scheduler service client
    pub async fn scheduler(&self) -> Result<SchedulerServiceClient<Channel>, GrpcClientError> {
        let channel = self.get_channel().await?;
        Ok(SchedulerServiceClient::new(channel))
    }

    /// Disconnect from the server
    pub async fn disconnect(&self) {
        *self.channel.write().await = None;
        *self.state.write().await = ConnectionState::Disconnected;
    }
}

/// Reconnection manager for automatic connection monitoring
#[derive(Clone)]
pub struct ReconnectionManager {
    client: GrpcClient,
    _handle: Arc<tokio::task::JoinHandle<()>>,
}

impl ReconnectionManager {
    /// Create a new reconnection manager
    pub fn new(client: GrpcClient, check_interval: Duration) -> Self {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let mut interval = interval(check_interval);

            loop {
                interval.tick().await;

                if client_clone.state().await == ConnectionState::Disconnected {
                    if let Err(e) = client_clone.connect_with_retry().await {
                        error!("Failed to reconnect: {}", e);
                    }
                }
            }
        });

        Self {
            client,
            _handle: Arc::new(handle),
        }
    }

    /// Stop the reconnection manager
    pub async fn stop(&self) {
        self.client.disconnect().await;
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

        let mut svc = self.client.jobs().await?;
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

        let mut svc = self.client.jobs().await?;
        let response = svc
            .get_job(request)
            .await
            .map_err(GrpcClientError::Request)?;

        Ok(response.into_inner())
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

    /// Get queue status
    pub async fn get_queue_status(&self) -> Result<hodei_jobs::QueueStatus, GrpcClientError> {
        let mut svc = self.client.scheduler().await?;
        let request = hodei_jobs::GetQueueStatusRequest {
            scheduler_name: self.client.config.scheduler_name.clone(),
        };
        let response = svc
            .get_queue_status(request)
            .await
            .map_err(GrpcClientError::Request)?;

        Ok(response.into_inner().status.unwrap_or_default())
    }
}
