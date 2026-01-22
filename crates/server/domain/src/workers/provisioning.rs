//! Worker Provisioning Domain Trait
//!
//! This module defines the domain-level trait for worker provisioning operations.
//! It's designed to be used by saga steps for real infrastructure management
//! during execution and compensation.
//!
//! # Why a domain trait?
//!
//! The application layer has `WorkerProvisioningService` with full business logic.
//! This domain trait provides a minimal interface for saga steps to perform
//! infrastructure operations without coupling to application services.
//!
//! This follows the Dependency Inversion Principle - the domain defines
//! what it needs, and the application layer provides an implementation.

use crate::shared_kernel::{JobId, ProviderId, Result, WorkerId};
use crate::workers::WorkerSpec;
use async_trait::async_trait;

/// Result of a worker provisioning operation.
///
/// This is a domain type used by the saga to track created workers
/// for potential compensation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerProvisioningResult {
    /// The ID of the provisioned worker
    pub worker_id: WorkerId,
    /// The provider used
    pub provider_id: ProviderId,
    /// The job this worker is associated with
    pub job_id: JobId,
}

impl WorkerProvisioningResult {
    /// Creates a new provisioning result.
    #[inline]
    pub fn new(worker_id: WorkerId, provider_id: ProviderId, job_id: JobId) -> Self {
        Self {
            worker_id,
            provider_id,
            job_id,
        }
    }

    /// Returns the worker ID as a string for metadata storage.
    #[inline]
    pub fn worker_id_str(&self) -> String {
        self.worker_id.to_string()
    }
}

/// Domain trait for worker provisioning operations.
///
/// This trait defines the minimal interface required by saga steps
/// to perform infrastructure operations. It complements the application
/// layer's `WorkerProvisioningService` by providing a domain-focused
/// interface for saga coordination.
///
/// # Operations
///
/// - `provision_worker`: Creates a new worker and returns its ID
/// - `destroy_worker`: Destroys an existing worker (for compensation)
/// - `is_provider_available`: Checks if a provider can create workers
///
/// # Error Handling
///
/// All methods return `Result` to allow the saga orchestrator to handle
/// failures and trigger compensation.
#[async_trait]
pub trait WorkerProvisioning: Send + Sync {
    /// Provisions (creates) a new worker on the specified provider.
    ///
    /// This is called during saga execution to create real infrastructure.
    /// The created worker ID is stored in the saga context for potential
    /// compensation if a later step fails.
    ///
    /// # Arguments
    /// * `provider_id` - The provider to use for worker creation
    /// * `spec` - Worker specification (image, resources, etc.)
    /// * `job_id` - The job this worker is dedicated to
    ///
    /// # Returns
    /// * `Ok(WorkerProvisioningResult)` - Contains the created worker ID
    /// * `Err(DomainError)` - If provisioning fails, saga will trigger compensation
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<WorkerProvisioningResult>;

    /// Destroys a previously provisioned worker.
    ///
    /// This is called during saga compensation to clean up infrastructure
    /// when a later step fails. It should be idempotent - destroying an
    /// already-destroyed worker should succeed.
    ///
    /// # Arguments
    /// * `worker_id` - The ID of the worker to destroy
    ///
    /// # Returns
    /// * `Ok(())` - Worker destroyed successfully
    /// * `Err(DomainError)` - If destruction fails, compensation may be incomplete
    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()>;

    /// Terminates a worker with a reason.
    ///
    /// This is used by v4 workflows (cancellation, timeout, cleanup) to
    /// gracefully terminate workers and report the reason for termination.
    /// Similar to `destroy_worker` but includes a reason parameter for logging.
    ///
    /// # Arguments
    /// * `worker_id` - The ID of the worker to terminate
    /// * `reason` - The reason for termination (for logging/auditing)
    ///
    /// # Returns
    /// * `Ok(())` - Worker terminated successfully
    /// * `Err(DomainError)` - If termination fails
    async fn terminate_worker(&self, worker_id: &WorkerId, reason: &str) -> Result<()>;

    /// Checks if a provider is available for worker provisioning.
    ///
    /// This can be used by saga steps to validate provider availability
    /// before attempting to provision a worker.
    ///
    /// # Arguments
    /// * `provider_id` - The provider to check
    ///
    /// # Returns
    /// * `Ok(true)` - Provider is available
    /// * `Ok(false)` - Provider is not available
    /// * `Err(DomainError)` - Failed to check provider status
    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::ProviderId;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock implementation for testing saga steps
    #[derive(Debug, Default)]
    struct MockWorkerProvisioning {
        provisioned: Arc<Mutex<Vec<WorkerProvisioningResult>>>,
        destroyed: Arc<Mutex<Vec<WorkerId>>>,
        available: Arc<Mutex<bool>>,
    }

    impl MockWorkerProvisioning {
        fn new() -> Self {
            Self {
                provisioned: Arc::new(Mutex::new(Vec::new())),
                destroyed: Arc::new(Mutex::new(Vec::new())),
                available: Arc::new(Mutex::new(true)),
            }
        }

        /// Create a provisioning service with custom availability
        fn with_availability(available: bool) -> Self {
            Self {
                provisioned: Arc::new(Mutex::new(Vec::new())),
                destroyed: Arc::new(Mutex::new(Vec::new())),
                available: Arc::new(Mutex::new(available)),
            }
        }
    }

    #[async_trait]
    impl WorkerProvisioning for MockWorkerProvisioning {
        async fn provision_worker(
            &self,
            provider_id: &ProviderId,
            _spec: WorkerSpec,
            job_id: JobId,
        ) -> Result<WorkerProvisioningResult> {
            let worker_id = WorkerId::new();
            let result = WorkerProvisioningResult::new(worker_id, provider_id.clone(), job_id);
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

    /// Helper to create a test provisioning result
    fn create_test_provisioning_result() -> WorkerProvisioningResult {
        WorkerProvisioningResult::new(WorkerId::new(), ProviderId::new(), JobId::new())
    }

    // ============ WorkerProvisioningResult Tests ============

    #[test]
    fn worker_provisioning_result_should_store_all_fields() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let job_id = JobId::new();

        let result =
            WorkerProvisioningResult::new(worker_id.clone(), provider_id.clone(), job_id.clone());

        assert_eq!(result.worker_id, worker_id);
        assert_eq!(result.provider_id, provider_id);
        assert_eq!(result.job_id, job_id);
    }

    #[test]
    fn worker_provisioning_result_worker_id_str() {
        let worker_id = WorkerId::new();
        let result =
            WorkerProvisioningResult::new(worker_id.clone(), ProviderId::new(), JobId::new());

        assert_eq!(result.worker_id_str(), worker_id.to_string());
    }

    #[test]
    fn worker_provisioning_result_should_be_cloneable() {
        let result = create_test_provisioning_result();
        let cloned = result.clone();

        assert_eq!(result.worker_id, cloned.worker_id);
        assert_eq!(result.provider_id, cloned.provider_id);
        assert_eq!(result.job_id, cloned.job_id);
    }

    // ============ Mock Implementation Tests ============

    #[tokio::test]
    async fn mock_provision_worker_should_store_result() {
        let mock = MockWorkerProvisioning::new();
        let provider_id = ProviderId::new();
        let job_id = JobId::new();
        let spec = WorkerSpec::new("alpine:latest".to_string(), "http://localhost".to_string());

        let result = mock
            .provision_worker(&provider_id, spec, job_id)
            .await
            .unwrap();

        assert!(mock.provisioned.lock().await.contains(&result));
    }

    #[tokio::test]
    async fn mock_destroy_worker_should_record_id() {
        let mock = MockWorkerProvisioning::new();
        let worker_id = WorkerId::new();

        mock.destroy_worker(&worker_id).await.unwrap();

        assert!(mock.destroyed.lock().await.contains(&worker_id));
    }

    #[tokio::test]
    async fn mock_is_provider_available_should_return_configured_value() {
        let available_mock = MockWorkerProvisioning::with_availability(true);
        let unavailable_mock = MockWorkerProvisioning::with_availability(false);

        let provider_id = ProviderId::new();

        assert!(
            available_mock
                .is_provider_available(&provider_id)
                .await
                .unwrap()
        );
        assert!(
            !unavailable_mock
                .is_provider_available(&provider_id)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn mock_destroy_worker_should_be_idempotent() {
        let mock = MockWorkerProvisioning::new();
        let worker_id = WorkerId::new();

        // Destroy twice
        mock.destroy_worker(&worker_id).await.unwrap();
        mock.destroy_worker(&worker_id).await.unwrap();

        // Should only be recorded once (logic wise) but twice in the list
        // In real implementations, this would be a set
        assert_eq!(mock.destroyed.lock().await.len(), 2);
    }

    // ============ Debug and Clone Tests ============

    #[test]
    fn worker_provisioning_result_should_be_debug() {
        let result = create_test_provisioning_result();
        let debug_str = format!("{:?}", result);
        assert!(!debug_str.is_empty());
    }
}
