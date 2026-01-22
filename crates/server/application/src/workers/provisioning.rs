//! Worker Provisioning Service - ISP Refactored
//!
//! Application layer ports for provisioning workers on-demand.
//! This module follows Interface Segregation Principle (ISP) by separating
//! concerns into focused traits:
//! - `WorkerProvisioningService`: Combined port for backward compatibility
//! - `WorkerProvisioner`: Core provisioning operations
//! - `WorkerProviderQuery`: Read-only queries for provider information
//! - `WorkerSpecValidator`: Specification validation
//! - `WorkerProvisioningConfig`: Provider configuration access

use async_trait::async_trait;
use hodei_server_domain::providers::ProviderConfig;
use hodei_server_domain::shared_kernel::{JobId, ProviderId, Result, WorkerId};
use hodei_server_domain::workers::WorkerSpec;
use std::sync::Arc;
use tokio::sync::Mutex;

// =============================================================================
// Result Types
// =============================================================================

/// Result of a successful worker provisioning (Application Layer)
///
/// Unlike the domain layer's `WorkerProvisioningResult`, this includes
/// the OTP token generated during provisioning.
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

// =============================================================================
// Segregated Traits (ISP Compliance)
// =============================================================================

/// Core provisioning operations - Part of WorkerProvisioningService
///
/// This trait focuses ONLY on worker lifecycle operations:
/// - Creating new workers
/// - Destroying workers
/// - Terminating workers
///
/// Separating this allows clients that only need provisioning to depend
/// on a smaller interface.
#[async_trait]
pub trait WorkerProvisioner: Send + Sync {
    /// Provision a new worker using the specified provider
    ///
    /// This method:
    /// 1. Creates the worker via the provider
    /// 2. Registers it in the WorkerRegistry with the job association
    /// 3. Generates an OTP token for authentication
    /// 4. Returns the provisioning result
    ///
    /// The job_id is REQUIRED - each worker is dedicated to a specific job.
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<ProvisioningResult>;

    /// Terminate a running worker
    async fn terminate_worker(&self, worker_id: &WorkerId, reason: &str) -> Result<()>;

    /// Destroy a worker's infrastructure
    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()>;
}

/// Query operations for provider information - Part of WorkerProvisioningService
///
/// This trait focuses ONLY on read operations for provider information.
/// Clients that only need to query providers can depend on this smaller interface.
#[async_trait]
pub trait WorkerProviderQuery: Send + Sync {
    /// Check if a provider is available for provisioning
    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool>;

    /// Get the default worker spec for a provider
    async fn default_worker_spec(&self, provider_id: &ProviderId) -> Option<WorkerSpec>;

    /// List all available providers
    async fn list_providers(&self) -> Result<Vec<ProviderId>>;

    /// Get provider configuration by ID
    async fn get_provider_config(&self, provider_id: &ProviderId)
    -> Result<Option<ProviderConfig>>;
}

/// Validation operations - Part of WorkerProvisioningService
///
/// This trait focuses ONLY on validation logic.
/// Clients that only need validation can depend on this smaller interface.
#[async_trait]
pub trait WorkerSpecValidator: Send + Sync {
    /// Validate a worker specification
    async fn validate_spec(&self, spec: &WorkerSpec) -> Result<()>;
}

// =============================================================================
// Combined Port (Backward Compatibility)
// =============================================================================

/// Port for provisioning workers (Combined Trait)
///
/// This trait combines all segregated traits for backward compatibility.
/// New code should depend on the specific segregated traits instead.
///
/// # Deprecation Notice
///
/// For new code, prefer depending on the specific traits:
/// - Use `WorkerProvisioner` if you only need provisioning operations
/// - Use `WorkerProviderQuery` if you only need to query providers
/// - Use `WorkerSpecValidator` if you only need validation
#[async_trait]
pub trait WorkerProvisioningService:
    WorkerProvisioner + WorkerProviderQuery + WorkerSpecValidator + Send + Sync
{
}

// =============================================================================
// Mock Implementation
// =============================================================================

/// Mock implementation for testing
///
/// This mock implements all the segregated traits for comprehensive testing.
pub struct MockProvisioningService {
    provisions: Arc<Mutex<Vec<(ProviderId, WorkerSpec)>>>,
    available_providers: Vec<ProviderId>,
}

impl MockProvisioningService {
    pub fn new(available_providers: Vec<ProviderId>) -> Self {
        Self {
            provisions: Arc::new(Mutex::new(Vec::new())),
            available_providers,
        }
    }
}

// Implement WorkerProvisioner for Mock
#[async_trait]
impl WorkerProvisioner for MockProvisioningService {
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

    async fn terminate_worker(&self, _worker_id: &WorkerId, _reason: &str) -> Result<()> {
        Ok(())
    }

    async fn destroy_worker(&self, _worker_id: &WorkerId) -> Result<()> {
        Ok(())
    }
}

// Implement WorkerProviderQuery for Mock
#[async_trait]
impl WorkerProviderQuery for MockProvisioningService {
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
}

// Implement WorkerSpecValidator for Mock
#[async_trait]
impl WorkerSpecValidator for MockProvisioningService {
    async fn validate_spec(&self, _spec: &WorkerSpec) -> Result<()> {
        Ok(())
    }
}

// Implement combined trait for backward compatibility
#[async_trait]
impl WorkerProvisioningService for MockProvisioningService {
    // All methods are automatically implemented via the supertraits
}

// =============================================================================
// Domain Layer Mocks (for saga testing)
// =============================================================================

/// Mock implementation of WorkerProvisioning (domain trait) for testing saga workflows
#[derive(Debug, Default)]
pub struct MockWorkerProvisioning {
    provisioned: Arc<Mutex<Vec<hodei_server_domain::workers::WorkerProvisioningResult>>>,
    destroyed: Arc<Mutex<Vec<WorkerId>>>,
    available: Arc<Mutex<bool>>,
}

impl MockWorkerProvisioning {
    pub fn new() -> Self {
        Self {
            provisioned: Arc::new(Mutex::new(Vec::new())),
            destroyed: Arc::new(Mutex::new(Vec::new())),
            available: Arc::new(Mutex::new(true)),
        }
    }

    /// Create a provisioning service with custom availability
    pub fn with_availability(available: bool) -> Self {
        Self {
            provisioned: Arc::new(Mutex::new(Vec::new())),
            destroyed: Arc::new(Mutex::new(Vec::new())),
            available: Arc::new(Mutex::new(available)),
        }
    }
}

#[async_trait::async_trait]
impl hodei_server_domain::workers::WorkerProvisioning for MockWorkerProvisioning {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        _spec: hodei_server_domain::workers::WorkerSpec,
        job_id: JobId,
    ) -> Result<hodei_server_domain::workers::WorkerProvisioningResult> {
        let worker_id = WorkerId::new();
        let result = hodei_server_domain::workers::WorkerProvisioningResult::new(
            worker_id,
            provider_id.clone(),
            job_id,
        );
        self.provisioned.lock().await.push(result.clone());
        Ok(result)
    }

    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()> {
        self.destroyed.lock().await.push(worker_id.clone());
        Ok(())
    }

    async fn terminate_worker(&self, worker_id: &WorkerId, _reason: &str) -> Result<()> {
        self.destroyed.lock().await.push(worker_id.clone());
        Ok(())
    }

    async fn is_provider_available(&self, _provider_id: &ProviderId) -> Result<bool> {
        Ok(*self.available.lock().await)
    }
}

// Implement application layer traits for MockWorkerProvisioning
// This allows it to be used in tests that expect WorkerProvisioningService

#[async_trait]
impl WorkerProvisioner for MockWorkerProvisioning {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<ProvisioningResult> {
        // Delegate to domain trait implementation
        let domain_result = hodei_server_domain::workers::WorkerProvisioning::provision_worker(
            self,
            provider_id,
            spec,
            job_id,
        )
        .await?;
        Ok(ProvisioningResult::new(
            domain_result.worker_id,
            "mock-otp".to_string(),
            domain_result.provider_id,
        ))
    }

    async fn terminate_worker(&self, worker_id: &WorkerId, reason: &str) -> Result<()> {
        hodei_server_domain::workers::WorkerProvisioning::terminate_worker(self, worker_id, reason)
            .await
    }

    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()> {
        hodei_server_domain::workers::WorkerProvisioning::destroy_worker(self, worker_id).await
    }
}

#[async_trait]
impl WorkerProviderQuery for MockWorkerProvisioning {
    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
        hodei_server_domain::workers::WorkerProvisioning::is_provider_available(self, provider_id)
            .await
    }

    async fn default_worker_spec(&self, _provider_id: &ProviderId) -> Option<WorkerSpec> {
        Some(WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        ))
    }

    async fn list_providers(&self) -> Result<Vec<ProviderId>> {
        Ok(vec![])
    }

    async fn get_provider_config(
        &self,
        _provider_id: &ProviderId,
    ) -> Result<Option<ProviderConfig>> {
        Ok(None)
    }
}

#[async_trait]
impl WorkerSpecValidator for MockWorkerProvisioning {
    async fn validate_spec(&self, _spec: &WorkerSpec) -> Result<()> {
        Ok(())
    }
}

// Implement combined trait for backward compatibility
#[async_trait]
impl WorkerProvisioningService for MockWorkerProvisioning {
    // All methods are automatically implemented via the supertraits
}

/// Mock implementation of WorkerRegistry for testing saga workflows
#[derive(Debug, Default)]
pub struct MockWorkerRegistry;

impl MockWorkerRegistry {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl hodei_server_domain::workers::WorkerRegistry for MockWorkerRegistry {
    async fn register(
        &self,
        _handle: hodei_server_domain::workers::WorkerHandle,
        _spec: hodei_server_domain::workers::WorkerSpec,
        _job_id: JobId,
    ) -> Result<hodei_server_domain::workers::Worker> {
        Err(
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: "Not implemented".to_string(),
            },
        )
    }
    async fn save(&self, _worker: &hodei_server_domain::workers::Worker) -> Result<()> {
        Ok(())
    }
    async fn unregister(&self, _worker_id: &WorkerId) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _id: &WorkerId,
    ) -> Result<Option<hodei_server_domain::workers::Worker>> {
        Ok(None)
    }
    async fn get_by_job_id(
        &self,
        _job_id: &JobId,
    ) -> Result<Option<hodei_server_domain::workers::Worker>> {
        Ok(None)
    }
    async fn find_by_job(
        &self,
        _job_id: &JobId,
    ) -> Result<Option<hodei_server_domain::workers::Worker>> {
        Ok(None)
    }
    async fn find(
        &self,
        _filter: &hodei_server_domain::workers::WorkerFilter,
    ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn find_ready_worker(
        &self,
        _filter: Option<&hodei_server_domain::workers::WorkerFilter>,
    ) -> Result<Option<hodei_server_domain::workers::Worker>> {
        Ok(None)
    }
    async fn update_state(
        &self,
        _worker_id: &WorkerId,
        _state: hodei_shared::states::WorkerState,
    ) -> Result<()> {
        Ok(())
    }
    async fn find_available(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn find_by_provider(
        &self,
        _provider_id: &ProviderId,
    ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn update_heartbeat(&self, _worker_id: &WorkerId) -> Result<()> {
        Ok(())
    }
    async fn mark_busy(&self, _worker_id: &WorkerId, _job_id: Option<JobId>) -> Result<()> {
        Ok(())
    }
    async fn release_from_job(&self, _worker_id: &WorkerId) -> Result<()> {
        Ok(())
    }
    async fn find_unhealthy(
        &self,
        _timeout: std::time::Duration,
    ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn find_for_termination(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn find_idle_timed_out(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn find_lifetime_exceeded(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn find_ttl_after_completion_exceeded(
        &self,
    ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
        Ok(vec![])
    }
    async fn count(&self) -> Result<usize> {
        Ok(0)
    }
    async fn stats(&self) -> Result<hodei_server_domain::workers::WorkerRegistryStats> {
        Ok(hodei_server_domain::workers::WorkerRegistryStats::default())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn test_terminate_worker() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);
        let worker_id = WorkerId::new();

        let result = service.terminate_worker(&worker_id, "test").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_destroy_worker() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);
        let worker_id = WorkerId::new();

        let result = service.destroy_worker(&worker_id).await;
        assert!(result.is_ok());
    }

    // Test that segregated traits work independently
    #[tokio::test]
    async fn test_worker_provisioner_trait_only() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);

        // Can use service as WorkerProvisioner only
        let provisioner: &dyn WorkerProvisioner = &service;

        let spec = WorkerSpec::new(
            "test:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        let result = provisioner
            .provision_worker(&provider_id, spec, JobId::new())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_worker_provider_query_trait_only() {
        let provider_id = ProviderId::new();
        let service = MockProvisioningService::new(vec![provider_id.clone()]);

        // Can use service as WorkerProviderQuery only
        let query: &dyn WorkerProviderQuery = &service;

        let available = query.is_provider_available(&provider_id).await.unwrap();
        assert!(available);

        let specs = query.list_providers().await.unwrap();
        assert!(!specs.is_empty());
    }

    #[tokio::test]
    async fn test_worker_spec_validator_trait_only() {
        let service = MockProvisioningService::new(vec![]);

        // Can use service as WorkerSpecValidator only
        let validator: &dyn WorkerSpecValidator = &service;

        let spec = WorkerSpec::new(
            "test:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        let result = validator.validate_spec(&spec).await;
        assert!(result.is_ok());
    }
}
