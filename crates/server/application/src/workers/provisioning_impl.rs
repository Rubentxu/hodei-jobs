//! Worker Provisioning Service Implementation
//!
//! Concrete implementation that uses WorkerLifecycleManager and OTP token store.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tracing::{info, warn};

use hodei_server_domain::iam::WorkerBootstrapTokenStore;
use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, Result, WorkerId};
use hodei_server_domain::workers::{
    WorkerProvider, WorkerProvisioning, WorkerProvisioningResult, WorkerRegistry, WorkerSpec,
};

use crate::providers::ProviderRegistry;
use crate::workers::provisioning::{ProvisioningResult, WorkerProvisioningService};

/// Default OTP TTL for provisioned workers (5 minutes)
const DEFAULT_OTP_TTL: Duration = Duration::from_secs(300);

/// Configuration for the provisioning service
#[derive(Debug, Clone)]
pub struct ProvisioningConfig {
    /// TTL for OTP tokens
    pub otp_ttl: Duration,
    /// Default worker image
    pub default_image: String,
    /// Server address for workers to connect to
    pub server_address: String,
    /// Maximum workers per provider
    pub max_workers_per_provider: usize,
}

impl Default for ProvisioningConfig {
    fn default() -> Self {
        Self {
            otp_ttl: DEFAULT_OTP_TTL,
            default_image: "hodei-jobs-worker:latest".to_string(),
            server_address: "http://localhost:50051".to_string(),
            max_workers_per_provider: 10,
        }
    }
}

impl ProvisioningConfig {
    pub fn new(server_address: String) -> Self {
        Self {
            server_address,
            ..Default::default()
        }
    }

    pub fn with_default_image(mut self, image: String) -> Self {
        self.default_image = image;
        self
    }

    pub fn with_otp_ttl(mut self, ttl: Duration) -> Self {
        self.otp_ttl = ttl;
        self
    }
}

/// Production implementation of WorkerProvisioningService
#[derive(Clone)]
pub struct DefaultWorkerProvisioningService {
    /// Worker registry for registration
    registry: Arc<dyn WorkerRegistry>,
    /// OTP token store for authentication
    token_store: Arc<dyn WorkerBootstrapTokenStore>,
    /// Available providers
    providers: Arc<DashMap<ProviderId, Arc<dyn WorkerProvider>>>,
    /// Provider registry for getting provider configs
    provider_registry: Arc<ProviderRegistry>,
    /// Configuration
    config: ProvisioningConfig,
}

impl std::fmt::Debug for DefaultWorkerProvisioningService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultWorkerProvisioningService")
            .field("config", &self.config)
            .finish()
    }
}

impl DefaultWorkerProvisioningService {
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        token_store: Arc<dyn WorkerBootstrapTokenStore>,
        providers: Arc<DashMap<ProviderId, Arc<dyn WorkerProvider>>>,
        provider_registry: Arc<ProviderRegistry>,
        config: ProvisioningConfig,
    ) -> Self {
        Self {
            registry,
            token_store,
            providers,
            provider_registry,
            config,
        }
    }

    /// Get provider by ID
    async fn get_provider(&self, provider_id: &ProviderId) -> Result<Arc<dyn WorkerProvider>> {
        self.providers
            .get(provider_id)
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: provider_id.clone(),
            })
            .map(|p| p.value().clone())
    }
}

#[async_trait]
impl WorkerProvisioningService for DefaultWorkerProvisioningService {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<ProvisioningResult> {
        info!(
            "Provisioning worker via provider {} with image {} for job {:?}",
            provider_id, spec.image, job_id
        );

        // Get the provider
        let provider = self.get_provider(provider_id).await?;

        // Check provider health
        let health =
            provider
                .health_check()
                .await
                .map_err(|e| DomainError::WorkerProvisioningFailed {
                    message: format!("Provider health check failed: {}", e),
                })?;

        if !matches!(
            health,
            hodei_server_domain::workers::HealthStatus::Healthy
                | hodei_server_domain::workers::HealthStatus::Degraded { .. }
        ) {
            return Err(DomainError::ProviderUnhealthy {
                provider_id: provider_id.clone(),
            });
        }

        // Generate OTP token BEFORE creating the container
        // This way the token can be passed to the worker via environment variables
        let worker_id = spec.worker_id.clone();
        let otp_token = self
            .token_store
            .issue(&worker_id, self.config.otp_ttl)
            .await?;

        // Create a mutable copy of the spec with OTP token and job_id in environment
        let mut spec_with_env = spec.clone();
        spec_with_env
            .environment
            .insert("HODEI_OTP_TOKEN".to_string(), otp_token.to_string());
        spec_with_env
            .environment
            .insert("HODEI_JOB_ID".to_string(), job_id.to_string());

        info!("Generated OTP for worker {}, creating container", worker_id);

        // Create worker via provider (now with OTP token in environment)
        let _handle = provider.create_worker(&spec_with_env).await.map_err(|e| {
            DomainError::WorkerProvisioningFailed {
                message: e.to_string(),
            }
        })?;

        // DO NOT register in registry here - worker will self-register via gRPC with OTP
        // When the worker starts, it will:
        // 1. Call register() gRPC endpoint with the OTP token
        // 2. Server validates OTP and registers worker in registry
        // 3. Server publishes WorkerRegistered event
        // 4. ProvisioningSaga continues with next steps

        info!(
            "Worker {} infrastructure provisioned successfully. Worker will self-register with OTP.",
            worker_id
        );

        Ok(ProvisioningResult::new(
            worker_id.clone(),
            otp_token.to_string(),
            provider_id.clone(),
        ))
    }

    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
        let Some(provider) = self.providers.get(provider_id) else {
            return Ok(false);
        };

        // Check health
        match provider.value().health_check().await {
            Ok(hodei_server_domain::workers::HealthStatus::Healthy) => Ok(true),
            Ok(hodei_server_domain::workers::HealthStatus::Degraded { reason }) => {
                warn!("Provider {} is degraded: {}", provider_id, reason);
                Ok(true) // Still available but degraded
            }
            Ok(_) => Ok(false),
            Err(e) => {
                warn!("Provider {} health check failed: {}", provider_id, e);
                Ok(false)
            }
        }
    }

    async fn default_worker_spec(&self, provider_id: &ProviderId) -> Option<WorkerSpec> {
        // GAP-006 FIX: Support HODEI_WORKER_IMAGE environment variable for worker image
        // This allows configuring the worker image without modifying provider configs
        let env_image = std::env::var("HODEI_WORKER_IMAGE").ok();

        // Get image from provider config, fallback to config default_image
        let result = self.provider_registry.get_provider(provider_id).await;
        match result {
            Ok(Some(config)) => {
                info!(
                    "✅ Found provider config for {}: name={}, type={:?}",
                    provider_id, config.name, config.provider_type
                );
                let image = match config.type_config {
                    hodei_server_domain::providers::ProviderTypeConfig::Docker(docker) => env_image
                        .clone()
                        .unwrap_or_else(|| docker.default_image.clone()),
                    hodei_server_domain::providers::ProviderTypeConfig::Kubernetes(k8s) => {
                        env_image
                            .clone()
                            .unwrap_or_else(|| k8s.default_image.clone())
                    }
                    _ => env_image
                        .clone()
                        .unwrap_or_else(|| self.config.default_image.clone()),
                };
                info!("🎯 Selected image for worker: {}", image);
                Some(WorkerSpec::new(image, self.config.server_address.clone()))
            }
            Ok(None) => {
                // GAP-006 FIX: Support HODEI_WORKER_IMAGE environment variable for worker image
                let image = env_image
                    .clone()
                    .unwrap_or_else(|| self.config.default_image.clone());
                warn!(
                    "⚠️ Provider {} not found in registry, using fallback image: {}",
                    provider_id, image
                );
                Some(WorkerSpec::new(image, self.config.server_address.clone()))
            }
            Err(e) => {
                // GAP-006 FIX: Support HODEI_WORKER_IMAGE environment variable for worker image
                let image = env_image
                    .clone()
                    .unwrap_or_else(|| self.config.default_image.clone());
                warn!(
                    "⚠️ Error getting provider {} from registry: {}, using fallback image: {}",
                    provider_id, e, image
                );
                Some(WorkerSpec::new(image, self.config.server_address.clone()))
            }
        }
    }

    async fn list_providers(&self) -> Result<Vec<ProviderId>> {
        Ok(self.providers.iter().map(|p| p.key().clone()).collect())
    }

    async fn get_provider_config(
        &self,
        provider_id: &ProviderId,
    ) -> Result<Option<hodei_server_domain::providers::ProviderConfig>> {
        let _provider = self.get_provider(provider_id).await?;
        // Convert WorkerProvider to ProviderConfig if possible
        // For now, return None as we need additional conversion logic
        Ok(None)
    }

    async fn validate_spec(&self, spec: &WorkerSpec) -> Result<()> {
        // Basic validation of worker spec
        if spec.image.is_empty() {
            return Err(DomainError::InvalidWorkerSpec {
                field: "image".to_string(),
                reason: "Worker image cannot be empty".to_string(),
            });
        }
        if spec.server_address.is_empty() {
            return Err(DomainError::InvalidWorkerSpec {
                field: "server_address".to_string(),
                reason: "Server address cannot be empty".to_string(),
            });
        }
        Ok(())
    }

    async fn terminate_worker(&self, worker_id: &WorkerId, reason: &str) -> Result<()> {
        info!("Terminating worker {} (reason: {})", worker_id, reason);

        // Get worker info to find the provider and handle
        let worker = self.registry.find_by_id(worker_id).await?.ok_or_else(|| {
            DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            }
        })?;

        let handle = worker.handle().clone();
        let provider_id = worker.provider_id().clone();

        // Get the provider
        let provider = self.get_provider(&provider_id).await?;

        // Try to destroy the worker on the provider
        match provider.destroy_worker(&handle).await {
            Ok(_) => {
                info!(
                    "Worker {} destroyed via provider {}",
                    worker_id, provider_id
                );
            }
            Err(e) => {
                warn!(
                    "Provider {} failed to destroy worker {}: {}",
                    provider_id, worker_id, e
                );
            }
        }

        // Unregister from registry
        self.registry.unregister(worker_id).await?;
        Ok(())
    }

    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()> {
        <Self as WorkerProvisioningService>::terminate_worker(self, worker_id, "saga compensation")
            .await
    }
}

/// Implementation of domain trait WorkerProvisioning for saga steps.
///
/// This allows the DefaultWorkerProvisioningService to be used directly
/// in saga steps like CreateInfrastructureStep.
#[async_trait]
impl WorkerProvisioning for DefaultWorkerProvisioningService {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<WorkerProvisioningResult> {
        // Delegate to WorkerProvisioningService implementation
        let result =
            WorkerProvisioningService::provision_worker(self, provider_id, spec, job_id.clone())
                .await?;
        Ok(WorkerProvisioningResult {
            worker_id: result.worker_id,
            provider_id: result.provider_id,
            job_id,
        })
    }

    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
        // Delegate to WorkerProvisioningService implementation
        WorkerProvisioningService::is_provider_available(self, provider_id).await
    }

    async fn terminate_worker(&self, worker_id: &WorkerId, reason: &str) -> Result<()> {
        <Self as WorkerProvisioningService>::terminate_worker(self, worker_id, reason).await
    }

    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()> {
        <Self as WorkerProvisioningService>::destroy_worker(self, worker_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provisioning_config_default() {
        let config = ProvisioningConfig::default();
        assert_eq!(config.otp_ttl, DEFAULT_OTP_TTL);
        assert_eq!(config.default_image, "hodei-jobs-worker:latest");
    }

    #[test]
    fn test_provisioning_config_builder() {
        let config = ProvisioningConfig::new("http://server:50051".to_string())
            .with_default_image("custom-worker:v1".to_string())
            .with_otp_ttl(Duration::from_secs(600));

        assert_eq!(config.server_address, "http://server:50051");
        assert_eq!(config.default_image, "custom-worker:v1");
        assert_eq!(config.otp_ttl, Duration::from_secs(600));
    }

    #[test]
    fn test_provisioning_result() {
        let worker_id = hodei_server_domain::shared_kernel::WorkerId::new();
        let provider_id = ProviderId::new();
        let result = ProvisioningResult::new(
            worker_id.clone(),
            "test-otp".to_string(),
            provider_id.clone(),
        );

        assert_eq!(result.worker_id, worker_id);
        assert_eq!(result.otp_token, "test-otp");
        assert_eq!(result.provider_id, provider_id);
    }
}
