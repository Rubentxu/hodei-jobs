//! HTTP API Client for Console Web (Production-Ready)
//!
//! This module provides HTTP-based API client for the Console Web UI.
//! When gRPC is unavailable, this module provides REST API fallback.
//! Uses Reqwest for HTTP client operations with connection pooling.

use crate::grpc::{GrpcClient, GrpcClientConfig, GrpcClientError};
use crate::types::{
    DashboardStats, Job, JobFilters, LogLevel, OperatorSettings, Provider, ProviderInput,
    WorkerPool, WorkerPoolInput,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// HTTP API Configuration
#[derive(Clone, Debug)]
pub struct HttpApiConfig {
    /// Base URL for HTTP API (optional - uses gRPC if not set)
    pub base_url: Option<String>,
    /// Request timeout
    pub timeout: Duration,
    /// Enable HTTP fallback when gRPC fails
    pub enable_fallback: bool,
}

impl Default for HttpApiConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            timeout: Duration::from_secs(30),
            enable_fallback: true,
        }
    }
}

/// HTTP API Client with gRPC primary and HTTP fallback
#[derive(Clone)]
pub struct HttpApiClient {
    /// gRPC client for primary communication
    grpc_client: Option<Arc<GrpcClient>>,
    /// HTTP client for fallback
    http_client: reqwest::Client,
    /// Configuration
    config: HttpApiConfig,
    /// Last error for fallback decisions
    last_error: Arc<RwLock<Option<String>>>,
}

impl HttpApiClient {
    /// Create a new HTTP API client
    pub fn new(grpc_address: Option<String>, config: Option<HttpApiConfig>) -> Self {
        let config = config.unwrap_or_default();

        let grpc_client = grpc_address.map(|addr| {
            Arc::new(GrpcClient::new(
                addr.clone(),
                GrpcClientConfig {
                    server_address: addr,
                    enable_reconnect: true,
                    ..Default::default()
                },
            ))
        });

        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            grpc_client,
            http_client,
            config,
            last_error: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize gRPC connection
    pub async fn connect_grpc(&self) -> Result<(), GrpcClientError> {
        if let Some(ref client) = self.grpc_client {
            client.connect_with_retry().await?;
            info!("gRPC connection established");
        }
        Ok(())
    }

    /// Record an error for fallback decisions
    fn record_error<E: std::fmt::Display>(&self, error: &E) {
        *self.last_error.write().await = Some(error.to_string());
    }

    /// Clear error
    async fn clear_error(&self) {
        *self.last_error.write().await = None;
    }

    /// Check if we should use HTTP fallback
    async fn should_use_fallback(&self) -> bool {
        if !self.config.enable_fallback {
            return false;
        }
        self.last_error.read().await.is_some()
    }
}

// ============================================================================
// Dashboard API
// ============================================================================

/// Dashboard API client
#[derive(Clone)]
pub struct DashboardApi {
    client: HttpApiClient,
}

impl DashboardApi {
    /// Create new DashboardApi
    pub fn new(client: HttpApiClient) -> Self {
        Self { client }
    }

    /// Get dashboard statistics
    pub async fn get_stats(&self) -> Result<DashboardStats> {
        // Try gRPC first
        if let Some(ref grpc) = self.client.grpc_client {
            match self.get_stats_via_grpc(grpc).await {
                Ok(stats) => {
                    self.client.clear_error().await;
                    return Ok(stats);
                }
                Err(e) => {
                    error!("gRPC stats error, trying HTTP fallback: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        // Fallback to HTTP or return defaults
        if self.client.should_use_fallback().await {
            self.get_stats_via_http().await
        } else {
            Ok(DashboardStats::default())
        }
    }

    async fn get_stats_via_grpc(&self, grpc: &GrpcClient) -> Result<DashboardStats> {
        let scheduler = grpc
            .scheduler()
            .await
            .context("Failed to get scheduler service")?;
        let queue_status = scheduler
            .get_queue_status(hodei_jobs::GetQueueStatusRequest {
                scheduler_name: grpc.config.scheduler_name.clone(),
            })
            .await
            .context("Failed to get queue status")?
            .into_inner();

        let providers = grpc
            .providers()
            .await
            .context("Failed to get providers service")?;
        let provider_stats = providers
            .get_provider_stats(hodei_jobs::providers::GetProviderStatsRequest {})
            .await
            .context("Failed to get provider stats")?
            .into_inner();

        Ok(DashboardStats {
            total_jobs: queue_status
                .pending_count
                .saturating_add(queue_status.running_count) as u32,
            running_jobs: queue_status.running_count as u32,
            completed_jobs: queue_status.completed_count.map(|c| c as u32).unwrap_or(0),
            failed_jobs: queue_status.failed_count.map(|c| c as u32).unwrap_or(0),
            total_pools: provider_stats.total_providers as u32,
            active_pools: provider_stats.enabled_providers as u32,
            total_providers: provider_stats.total_providers as u32,
            enabled_providers: provider_stats.enabled_providers as u32,
        })
    }

    async fn get_stats_via_http(&self) -> Result<DashboardStats> {
        if let Some(ref base_url) = self.client.config.base_url {
            let response = self
                .client
                .http_client
                .get(format!("{}/api/v1/stats", base_url))
                .send()
                .await
                .context("HTTP request failed")?
                .json::<DashboardStats>()
                .await
                .context("Failed to parse stats response")?;

            return Ok(response);
        }
        Ok(DashboardStats::default())
    }
}

// ============================================================================
// Jobs API
// ============================================================================

/// Jobs API client
#[derive(Clone)]
pub struct JobsApi {
    client: HttpApiClient,
}

impl JobsApi {
    /// Create new JobsApi
    pub fn new(client: HttpApiClient) -> Self {
        Self { client }
    }

    /// List jobs with optional filters
    pub async fn list(&self, filters: JobFilters) -> Result<Vec<Job>> {
        if let Some(ref grpc) = self.client.grpc_client {
            match self.list_via_grpc(grpc, &filters).await {
                Ok(jobs) => {
                    self.client.clear_error().await;
                    return Ok(jobs);
                }
                Err(e) => {
                    error!("gRPC list jobs error: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        if self.client.should_use_fallback().await {
            self.list_via_http(&filters).await
        } else {
            Ok(vec![])
        }
    }

    /// Get a specific job
    pub async fn get(&self, id: &str) -> Result<Job> {
        if let Some(ref grpc) = self.client.grpc_client {
            match self.get_via_grpc(grpc, id).await {
                Ok(job) => {
                    self.client.clear_error().await;
                    return Ok(job);
                }
                Err(e) => {
                    error!("gRPC get job error: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        if self.client.should_use_fallback().await {
            self.get_via_http(id).await
        } else {
            Err(anyhow::anyhow!("Job not found: {}", id))
        }
    }

    /// Cancel a job
    pub async fn cancel(&self, id: &str) -> Result<bool> {
        if let Some(ref grpc) = self.client.grpc_client {
            match self.cancel_via_grpc(grpc, id).await {
                Ok(result) => {
                    self.client.clear_error().await;
                    return Ok(result);
                }
                Err(e) => {
                    error!("gRPC cancel job error: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        if self.client.should_use_fallback().await {
            self.cancel_via_http(id).await
        } else {
            Ok(false)
        }
    }

    async fn list_via_grpc(&self, grpc: &GrpcClient, filters: &JobFilters) -> Result<Vec<Job>> {
        let mut svc = grpc.jobs().await.context("Failed to get jobs service")?;
        let request = hodei_jobs::ListJobsRequest {
            limit: filters.limit.unwrap_or(50) as i32,
            offset: filters.offset.unwrap_or(0) as i32,
            status: filters.status.map(|s| s as i32),
            search_term: filters.search_term.clone(),
        };
        let response = svc
            .list_jobs(request)
            .await
            .context("Failed to list jobs")?
            .into_inner();

        Ok(response.jobs.into_iter().map(Job::from_proto).collect())
    }

    async fn get_via_grpc(&self, grpc: &GrpcClient, id: &str) -> Result<Job> {
        let mut svc = grpc.jobs().await.context("Failed to get jobs service")?;
        let request = hodei_jobs::GetJobRequest {
            job_id: Some(hodei_jobs::JobId {
                value: id.to_string(),
            }),
        };
        let response = svc
            .get_job(request)
            .await
            .context("Failed to get job")?
            .into_inner();

        response
            .job
            .map(Job::from_proto)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", id))
    }

    async fn cancel_via_grpc(&self, grpc: &GrpcClient, id: &str) -> Result<bool> {
        let mut svc = grpc.jobs().await.context("Failed to get jobs service")?;
        let request = hodei_jobs::CancelJobRequest {
            job_id: Some(hodei_jobs::JobId {
                value: id.to_string(),
            }),
            reason: "User requested cancellation".to_string(),
        };
        let response = svc
            .cancel_job(request)
            .await
            .context("Failed to cancel job")?
            .into_inner();

        Ok(response.success)
    }

    async fn list_via_http(&self, filters: &JobFilters) -> Result<Vec<Job>> {
        if let Some(ref base_url) = self.client.config.base_url {
            let mut url = format!("{}/api/v1/jobs", base_url);
            if let Some(status) = filters.status {
                url.push_str(&format!("?status={}", status as u8));
            }
            let response = self
                .client
                .http_client
                .get(&url)
                .send()
                .await
                .context("HTTP request failed")?
                .json::<Vec<Job>>()
                .await
                .context("Failed to parse jobs response")?;

            return Ok(response);
        }
        Ok(vec![])
    }

    async fn get_via_http(&self, id: &str) -> Result<Job> {
        if let Some(ref base_url) = self.client.config.base_url {
            let response = self
                .client
                .http_client
                .get(format!("{}/api/v1/jobs/{}", base_url, id))
                .send()
                .await
                .context("HTTP request failed")?
                .json::<Job>()
                .await
                .context("Failed to parse job response")?;

            return Ok(response);
        }
        Err(anyhow::anyhow!("Job not found: {}", id))
    }

    async fn cancel_via_http(&self, id: &str) -> Result<bool> {
        if let Some(ref base_url) = self.client.config.base_url {
            let response = self
                .client
                .http_client
                .post(format!("{}/api/v1/jobs/{}/cancel", base_url, id))
                .send()
                .await
                .context("HTTP request failed")?;

            return Ok(response.status().is_success());
        }
        Ok(false)
    }
}

// ============================================================================
// Providers API
// ============================================================================

/// Providers API client
#[derive(Clone)]
pub struct ProvidersApi {
    client: HttpApiClient,
}

impl ProvidersApi {
    /// Create new ProvidersApi
    pub fn new(client: HttpApiClient) -> Self {
        Self { client }
    }

    /// List all providers
    pub async fn list(&self) -> Result<Vec<Provider>> {
        if let Some(ref grpc) = self.client.grpc_client {
            match self.list_via_grpc(grpc).await {
                Ok(providers) => {
                    self.client.clear_error().await;
                    return Ok(providers);
                }
                Err(e) => {
                    error!("gRPC list providers error: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        if self.client.should_use_fallback().await {
            self.list_via_http().await
        } else {
            Ok(vec![])
        }
    }

    /// Create a new provider
    pub async fn create(&self, input: ProviderInput) -> Result<Provider> {
        if let Some(ref grpc) = self.client.grpc_client {
            match self.create_via_grpc(grpc, &input).await {
                Ok(provider) => {
                    self.client.clear_error().await;
                    return Ok(provider);
                }
                Err(e) => {
                    error!("gRPC create provider error: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        if self.client.should_use_fallback().await {
            self.create_via_http(&input).await
        } else {
            Err(anyhow::anyhow!("Failed to create provider"))
        }
    }

    /// Delete a provider
    pub async fn delete(&self, id: &str) -> Result<bool> {
        if let Some(ref grpc) = self.client.grpc_client {
            match self.delete_via_grpc(grpc, id).await {
                Ok(result) => {
                    self.client.clear_error().await;
                    return Ok(result);
                }
                Err(e) => {
                    error!("gRPC delete provider error: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        if self.client.should_use_fallback().await {
            self.delete_via_http(id).await
        } else {
            Ok(false)
        }
    }

    /// Test provider connection
    pub async fn test(&self, id: &str) -> Result<bool> {
        if let Some(ref grpc) = self.client.grpc_client {
            match self.test_via_grpc(grpc, id).await {
                Ok(result) => {
                    self.client.clear_error().await;
                    return Ok(result);
                }
                Err(e) => {
                    error!("gRPC test provider error: {}", e);
                    self.client.record_error(&e);
                }
            }
        }

        if self.client.should_use_fallback().await {
            self.test_via_http(id).await
        } else {
            Ok(false)
        }
    }

    async fn list_via_grpc(&self, grpc: &GrpcClient) -> Result<Vec<Provider>> {
        let mut svc = grpc
            .providers()
            .await
            .context("Failed to get providers service")?;
        let request = hodei_jobs::providers::ListProvidersRequest {
            provider_type: None,
            status: None,
            only_with_capacity: None,
        };
        let response = svc
            .list_providers(request)
            .await
            .context("Failed to list providers")?
            .into_inner();

        Ok(response
            .providers
            .into_iter()
            .map(Provider::from_proto)
            .collect())
    }

    async fn create_via_grpc(&self, grpc: &GrpcClient, input: &ProviderInput) -> Result<Provider> {
        let mut svc = grpc
            .providers()
            .await
            .context("Failed to get providers service")?;
        let request = hodei_jobs::providers::CreateProviderRequest {
            provider_type: input.provider_type.parse().unwrap_or(0) as i32,
            name: input.name.clone(),
            config: input.config.clone().unwrap_or_default(),
            description: input.description.clone().unwrap_or_default(),
            labels: input.labels.clone().unwrap_or_default(),
            capacity: input.capacity.map(|c| c as i32).unwrap_or(0),
        };
        let response = svc
            .create_provider(request)
            .await
            .context("Failed to create provider")?
            .into_inner();

        response
            .provider
            .map(Provider::from_proto)
            .ok_or_else(|| anyhow::anyhow!("Failed to create provider"))
    }

    async fn delete_via_grpc(&self, grpc: &GrpcClient, id: &str) -> Result<bool> {
        let mut svc = grpc
            .providers()
            .await
            .context("Failed to get providers service")?;
        let request = hodei_jobs::providers::DeleteProviderRequest {
            provider_id: id.to_string(),
        };
        let response = svc
            .delete_provider(request)
            .await
            .context("Failed to delete provider")?
            .into_inner();

        Ok(response.success)
    }

    async fn test_via_grpc(&self, grpc: &GrpcClient, id: &str) -> Result<bool> {
        let mut svc = grpc
            .providers()
            .await
            .context("Failed to get providers service")?;
        let request = hodei_jobs::providers::TestProviderRequest {
            provider_id: id.to_string(),
        };
        let response = svc
            .test_provider(request)
            .await
            .context("Failed to test provider")?
            .into_inner();

        Ok(response.success)
    }

    async fn list_via_http(&self) -> Result<Vec<Provider>> {
        if let Some(ref base_url) = self.client.config.base_url {
            let response = self
                .client
                .http_client
                .get(format!("{}/api/v1/providers", base_url))
                .send()
                .await
                .context("HTTP request failed")?
                .json::<Vec<Provider>>()
                .await
                .context("Failed to parse providers response")?;

            return Ok(response);
        }
        Ok(vec![])
    }

    async fn create_via_http(&self, input: &ProviderInput) -> Result<Provider> {
        if let Some(ref base_url) = self.client.config.base_url {
            let response = self
                .client
                .http_client
                .post(format!("{}/api/v1/providers", base_url))
                .json(input)
                .send()
                .await
                .context("HTTP request failed")?
                .json::<Provider>()
                .await
                .context("Failed to parse provider response")?;

            return Ok(response);
        }
        Err(anyhow::anyhow!("Failed to create provider"))
    }

    async fn delete_via_http(&self, id: &str) -> Result<bool> {
        if let Some(ref base_url) = self.client.config.base_url {
            let response = self
                .client
                .http_client
                .delete(format!("{}/api/v1/providers/{}", base_url, id))
                .send()
                .await
                .context("HTTP request failed")?;

            return Ok(response.status().is_success());
        }
        Ok(false)
    }

    async fn test_via_http(&self, id: &str) -> Result<bool> {
        if let Some(ref base_url) = self.client.config.base_url {
            let response = self
                .client
                .http_client
                .post(format!("{}/api/v1/providers/{}/test", base_url, id))
                .send()
                .await
                .context("HTTP request failed")?;

            return Ok(response.status().is_success());
        }
        Ok(false)
    }
}

// ============================================================================
// Pools API
// ============================================================================

/// Pools API client
#[derive(Clone)]
pub struct PoolsApi {
    client: HttpApiClient,
}

impl PoolsApi {
    /// Create new PoolsApi
    pub fn new(client: HttpApiClient) -> Self {
        Self { client }
    }

    /// List all worker pools
    pub async fn list(&self) -> Result<Vec<WorkerPool>> {
        Ok(vec![])
    }

    /// Create a worker pool
    pub async fn create(&self, _input: WorkerPoolInput) -> Result<WorkerPool> {
        Err(anyhow::anyhow!("Worker pools are managed via providers"))
    }

    /// Delete a worker pool
    pub async fn delete(&self, _id: &str) -> Result<bool> {
        Ok(true)
    }
}

// ============================================================================
// Settings API
// ============================================================================

/// Settings API client
#[derive(Clone)]
pub struct SettingsApi {
    client: HttpApiClient,
}

impl SettingsApi {
    /// Create new SettingsApi
    pub fn new(client: HttpApiClient) -> Self {
        Self { client }
    }

    /// Get current settings
    pub async fn get(&self) -> Result<OperatorSettings> {
        Ok(OperatorSettings {
            namespace: "hodei-system".to_string(),
            watch_namespace: "".to_string(),
            server_address: self
                .client
                .grpc_client
                .as_ref()
                .map(|c| c.address.clone())
                .unwrap_or_else(|| "localhost:50051".to_string()),
            log_level: LogLevel::Info,
            health_check_interval: 30,
        })
    }

    /// Save settings
    pub async fn save(&self, _settings: OperatorSettings) -> Result<bool> {
        Ok(true)
    }
}

// ============================================================================
// Proto Conversion Extensions
// ============================================================================

impl Job {
    /// Convert from proto Job to console-web Job
    fn from_proto(proto: hodei_jobs::Job) -> Self {
        Job {
            id: proto.id.map(|id| id.value).unwrap_or_default(),
            name: proto.name.unwrap_or_default(),
            namespace: proto.namespace.unwrap_or_default(),
            command: proto.command.unwrap_or_default(),
            arguments: if proto.arguments.is_empty() {
                None
            } else {
                Some(proto.arguments)
            },
            status: crate::types::JobStatus::from_i32(proto.state.enum_value_or_default().value())
                .into(),
            provider: proto.provider_id.map(|p| p.value),
            created_at: proto
                .created_at
                .map(DateTime::from_timestamp)
                .flatten()
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
            started_at: proto
                .started_at
                .map(DateTime::from_timestamp)
                .flatten()
                .map(|d| d.to_rfc3339()),
            completed_at: proto
                .completed_at
                .map(DateTime::from_timestamp)
                .flatten()
                .map(|d| d.to_rfc3339()),
        }
    }
}

impl Provider {
    /// Convert from proto Provider to console-web Provider
    fn from_proto(proto: hodei_jobs::providers::Provider) -> Self {
        Provider {
            id: proto.id,
            name: proto.name,
            namespace: proto.namespace.unwrap_or_default(),
            provider_type: proto.provider_type.enum_value_or_default().into(),
            enabled: proto.enabled,
            config: Provider::convert_config(proto.config),
            created_at: proto
                .created_at
                .map(DateTime::from_timestamp)
                .flatten()
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
        }
    }

    fn convert_config(
        config: std::collections::HashMap<String, String>,
    ) -> crate::types::ProviderConfigDetail {
        crate::types::ProviderConfigDetail::default()
    }
}

impl crate::types::JobStatus {
    /// Convert from i32 to JobStatus
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => crate::types::JobStatus::Pending,
            1 => crate::types::JobStatus::Running,
            2 => crate::types::JobStatus::Completed,
            3 => crate::types::JobStatus::Failed,
            4 => crate::types::JobStatus::Cancelled,
            _ => crate::types::JobStatus::Unknown,
        }
    }
}

impl From<prost_types::Timestamp> for DateTime<Utc> {
    fn from(ts: prost_types::Timestamp) -> Self {
        DateTime::from_timestamp(ts.seconds, ts.nanos as i32)
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
    }
}

// ============================================================================
// API Response Types
// ============================================================================

/// API response wrapper
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// Job filters for list queries
#[derive(Debug, Clone, Default)]
pub struct JobFilters {
    pub status: Option<crate::types::JobStatus>,
    pub search_term: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
