//! Worker Provisioning Service
//!
//! Application layer port for provisioning workers on-demand.
//! This abstracts the infrastructure details of worker creation.

use async_trait::async_trait;
use hodei_server_domain::providers::ProviderConfig;
use hodei_server_domain::shared_kernel::{JobId, ProviderId, Result, WorkerId};
use hodei_server_domain::workers::WorkerSpec;

/// Result of a successful worker provisioning
#[derive(Debug, Clone)]
pub struct ProvisioningResult {
    /// The ID of the newly provisioned worker
    pub worker_id: WorkerId,
    /// The OTP token for the worker to authenticate
    pub otp_token: String,
    /// The provider that was used
    pub provider_id: ProviderId,
}

impl ProvisioningResult {
    pub fn new(worker_id: WorkerId, otp_token: String, provider_id: ProviderId) -> Self {
        Self {
            worker_id,
            otp_token,
            provider_id,
        }
    }
}

/// Port for provisioning workers
///
/// This trait defines the contract for provisioning new workers.
/// Implementations handle the actual infrastructure interaction
/// (Docker, Kubernetes, etc.) and OTP generation.
#[async_trait]
pub trait WorkerProvisioningService: Send + Sync {
    /// Provision a new worker using the specified provider
    ///
    /// This method:
    /// 1. Creates the worker via the provider
    /// 2. Registers it in the WorkerRegistry with the job association
    /// 3. Generates an OTP token for authentication
    /// 4. Returns the provisioning result
    ///
    /// The job_id is REQUIRED - each worker is dedicated to a specific job.
    /// This ensures proper worker-to-job matching as per the system policy.
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<ProvisioningResult>;

    /// Check if a provider is available for provisioning
    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool>;

    /// Get the default worker spec for a provider
    async fn default_worker_spec(&self, provider_id: &ProviderId) -> Option<WorkerSpec>;

    /// List all available providers
    async fn list_providers(&self) -> Result<Vec<ProviderId>>;

    /// Get provider configuration by ID
    async fn get_provider_config(&self, provider_id: &ProviderId)
    -> Result<Option<ProviderConfig>>;

    /// Validate a worker specification
    async fn validate_spec(&self, spec: &WorkerSpec) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock implementation for testing
    struct MockProvisioningService {
        provisions: Arc<Mutex<Vec<(ProviderId, WorkerSpec)>>>,
        available_providers: Vec<ProviderId>,
    }

    impl MockProvisioningService {
        fn new(available_providers: Vec<ProviderId>) -> Self {
            Self {
                provisions: Arc::new(Mutex::new(Vec::new())),
                available_providers,
            }
        }
    }

    #[async_trait]
    impl WorkerProvisioningService for MockProvisioningService {
        async fn provision_worker(
            &self,
            provider_id: &ProviderId,
            spec: WorkerSpec,
            _job_id: JobId,
        ) -> Result<ProvisioningResult> {
            self.provisions
                .lock()
                .await
                .push((provider_id.clone(), spec.clone()));
            Ok(ProvisioningResult::new(
                spec.worker_id,
                uuid::Uuid::new_v4().to_string(),
                provider_id.clone(),
            ))
        }

        async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
            Ok(self.available_providers.contains(provider_id))
        }

        async fn default_worker_spec(&self, _provider_id: &ProviderId) -> Option<WorkerSpec> {
            Some(WorkerSpec::new(
                "hodei-jobs-worker:latest".to_string(),
                "http://localhost:50051".to_string(),
            ))
        }

        async fn list_providers(&self) -> Result<Vec<ProviderId>> {
            Ok(self.available_providers.clone())
        }

        async fn get_provider_config(
            &self,
            _provider_id: &ProviderId,
        ) -> Result<Option<ProviderConfig>> {
            Ok(None)
        }

        async fn validate_spec(&self, _spec: &WorkerSpec) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_provision_worker_returns_result() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);

        let spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        let job_id = JobId::new();
        let result = service.provision_worker(&provider_id, spec, job_id).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.provider_id, provider_id);
        assert!(!result.otp_token.is_empty());
    }

    #[tokio::test]
    async fn test_is_provider_available() {
        let provider_id = ProviderId::new();
        let other_provider = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);

        assert!(service.is_provider_available(&provider_id).await.unwrap());
        assert!(
            !service
                .is_provider_available(&other_provider)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_default_worker_spec() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);

        let spec = service.default_worker_spec(&provider_id).await;
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().image, "hodei-jobs-worker:latest");
    }

    #[tokio::test]
    async fn test_get_provider_config() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);

        let config = service.get_provider_config(&provider_id).await;
        assert!(config.is_ok());
    }

    #[tokio::test]
    async fn test_validate_spec() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);

        let spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        let result = service.validate_spec(&spec).await;
        assert!(result.is_ok());
    }
}
