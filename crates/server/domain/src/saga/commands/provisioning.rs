// Provisioning Saga Commands
//
// Commands used by the ProvisioningSaga for worker lifecycle management.
// These commands encapsulate the intent to provision or destroy workers.

use crate::command::{Command, CommandHandler, CommandMetadataDefault};
use crate::shared_kernel::{JobId, ProviderId, WorkerId};
use crate::workers::{WorkerProvisioning, WorkerProvisioningResult, WorkerRegistry, WorkerSpec};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;

/// Command to provision a new worker.
///
/// This command encapsulates the intent to create infrastructure
/// for a worker. It is used by the ProvisioningSaga's CreateInfrastructureStep.
///
/// # Idempotency
///
/// The idempotency key is derived from the saga_id, ensuring that
/// retrying a saga will not create duplicate workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkerCommand {
    /// The worker specification (image, resources, etc.)
    pub spec: WorkerSpec,
    /// The provider to use for worker creation
    pub provider_id: ProviderId,
    /// The job this worker will be dedicated to
    pub job_id: JobId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing and context
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl CreateWorkerCommand {
    /// Creates a new CreateWorkerCommand.
    #[inline]
    pub fn new(spec: WorkerSpec, provider_id: ProviderId, job_id: JobId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            spec,
            provider_id,
            job_id,
            saga_id,
            metadata,
        }
    }

    /// Creates a command with custom metadata.
    #[inline]
    pub fn with_metadata(
        spec: WorkerSpec,
        provider_id: ProviderId,
        job_id: JobId,
        saga_id: String,
        metadata: CommandMetadataDefault,
    ) -> Self {
        Self {
            spec,
            provider_id,
            job_id,
            saga_id,
            metadata,
        }
    }
}

impl Command for CreateWorkerCommand {
    type Output = WorkerProvisioningResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}-create-worker", self.saga_id))
    }
}

/// Error types for CreateWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum CreateWorkerError {
    #[error("Provider {provider_id} is not available")]
    ProviderNotAvailable { provider_id: ProviderId },

    #[error("Failed to provision worker: {source}")]
    ProvisioningFailed {
        source: crate::shared_kernel::DomainError,
    },

    #[error("Worker provisioning was cancelled")]
    Cancelled,
}

/// Handler for CreateWorkerCommand.
///
/// This handler uses the domain's WorkerProvisioning trait to
/// provision actual infrastructure.
#[derive(Debug)]
pub struct CreateWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    provisioning: P,
}

impl<P> CreateWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    /// Creates a new handler with the given provisioning service.
    #[inline]
    pub fn new(provisioning: P) -> Self {
        Self { provisioning }
    }
}

#[async_trait]
impl<P> CommandHandler<CreateWorkerCommand> for CreateWorkerHandler<P>
where
    P: WorkerProvisioning + Debug + Send + Sync + 'static,
{
    type Error = CreateWorkerError;

    async fn handle(
        &self,
        command: CreateWorkerCommand,
    ) -> Result<WorkerProvisioningResult, Self::Error> {
        self.provisioning
            .provision_worker(&command.provider_id, command.spec, command.job_id)
            .await
            .map_err(|e| CreateWorkerError::ProvisioningFailed { source: e })
    }
}

/// Command to destroy a previously provisioned worker.
///
/// This command encapsulates the intent to clean up infrastructure.
/// It is used during saga compensation to undo worker provisioning.
///
/// # Idempotency
///
/// Destroy commands are inherently idempotent - destroying an
/// already-destroyed worker should succeed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestroyWorkerCommand {
    /// The ID of the worker to destroy
    pub worker_id: WorkerId,
    /// The provider used for the original worker
    pub provider_id: ProviderId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional reason for destruction (for logging)
    #[serde(default)]
    pub reason: Option<String>,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl DestroyWorkerCommand {
    /// Creates a new DestroyWorkerCommand.
    #[inline]
    pub fn new(worker_id: WorkerId, provider_id: ProviderId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            provider_id,
            saga_id,
            reason: None,
            metadata,
        }
    }

    /// Creates a command with a destruction reason.
    #[inline]
    pub fn with_reason(
        worker_id: WorkerId,
        provider_id: ProviderId,
        saga_id: String,
        reason: impl Into<String>,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            provider_id,
            saga_id,
            reason: Some(reason.into()),
            metadata,
        }
    }
}

impl Command for DestroyWorkerCommand {
    type Output = ();

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-destroy-worker-{}",
            self.saga_id, self.worker_id
        ))
    }
}

/// Error types for DestroyWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum DestroyWorkerError {
    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Failed to destroy worker {worker_id}: {source}")]
    DestructionFailed {
        worker_id: WorkerId,
        source: crate::shared_kernel::DomainError,
    },

    #[error("Worker destruction was cancelled")]
    Cancelled,
}

/// Handler for DestroyWorkerCommand.
#[derive(Debug)]
pub struct DestroyWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    provisioning: P,
}

impl<P> DestroyWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    /// Creates a new handler with the given provisioning service.
    #[inline]
    pub fn new(provisioning: P) -> Self {
        Self { provisioning }
    }
}

#[async_trait]
impl<P> CommandHandler<DestroyWorkerCommand> for DestroyWorkerHandler<P>
where
    P: WorkerProvisioning + Debug + Send + Sync + 'static,
{
    type Error = DestroyWorkerError;

    async fn handle(&self, command: DestroyWorkerCommand) -> Result<(), Self::Error> {
        self.provisioning
            .destroy_worker(&command.worker_id)
            .await
            .map_err(|e| DestroyWorkerError::DestructionFailed {
                worker_id: command.worker_id,
                source: e,
            })
    }
}

/// Command to unregister a worker from the registry.
///
/// This command removes the worker from the active registry.
/// It is typically used during cleanup or when a worker is destroyed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnregisterWorkerCommand {
    /// The ID of the worker to unregister
    pub worker_id: WorkerId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional reason for unregistration
    #[serde(default)]
    pub reason: Option<String>,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl UnregisterWorkerCommand {
    /// Creates a new UnregisterWorkerCommand.
    #[inline]
    pub fn new(worker_id: WorkerId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            saga_id,
            reason: None,
            metadata,
        }
    }

    /// Creates a command with a reason.
    #[inline]
    pub fn with_reason(
        worker_id: WorkerId,
        saga_id: String,
        reason: impl Into<String>,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            saga_id,
            reason: Some(reason.into()),
            metadata,
        }
    }
}

impl Command for UnregisterWorkerCommand {
    type Output = ();

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-unregister-worker-{}",
            self.saga_id, self.worker_id
        ))
    }
}

/// Error types for UnregisterWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum UnregisterWorkerError {
    #[error("Failed to unregister worker {worker_id}: {source}")]
    UnregistrationFailed {
        worker_id: WorkerId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for UnregisterWorkerCommand.
#[derive(Debug)]
pub struct UnregisterWorkerHandler<R>
where
    R: WorkerRegistry + Debug,
{
    registry: R,
}

impl<R> UnregisterWorkerHandler<R>
where
    R: WorkerRegistry + Debug,
{
    /// Creates a new handler with the given registry.
    #[inline]
    pub fn new(registry: R) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl<R> CommandHandler<UnregisterWorkerCommand> for UnregisterWorkerHandler<R>
where
    R: WorkerRegistry + Debug + Send + Sync + 'static,
{
    type Error = UnregisterWorkerError;

    async fn handle(&self, command: UnregisterWorkerCommand) -> Result<(), Self::Error> {
        self.registry
            .unregister(&command.worker_id)
            .await
            .map_err(|e| UnregisterWorkerError::UnregistrationFailed {
                worker_id: command.worker_id,
                source: e,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::{ProviderId, Result};
    use crate::workers::WorkerSpec;
    use async_trait::async_trait;
    use std::sync::Arc;

    // Test mock implementation
    #[derive(Debug)]
    struct MockWorkerProvisioning;

    #[async_trait]
    impl WorkerProvisioning for MockWorkerProvisioning {
        async fn provision_worker(
            &self,
            provider_id: &ProviderId,
            _spec: WorkerSpec,
            job_id: JobId,
        ) -> Result<WorkerProvisioningResult> {
            Ok(WorkerProvisioningResult::new(
                WorkerId::new(),
                provider_id.clone(),
                job_id,
            ))
        }

        async fn destroy_worker(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn is_provider_available(&self, _provider_id: &ProviderId) -> Result<bool> {
            Ok(true)
        }
    }

    fn make_test_worker_spec() -> WorkerSpec {
        WorkerSpec::new(
            "test-image:latest".to_string(),
            "localhost:50051".to_string(),
        )
    }

    #[tokio::test]
    async fn create_worker_command_idempotency_key() {
        let saga_id = "saga-123".to_string();
        let cmd = CreateWorkerCommand::new(
            make_test_worker_spec(),
            ProviderId::new(),
            JobId::new(),
            saga_id,
        );

        let key = cmd.idempotency_key();
        assert!(key.contains("saga-123"));
        assert!(key.contains("create-worker"));
    }

    #[tokio::test]
    async fn create_worker_command_with_metadata() {
        let metadata = CommandMetadataDefault::new().with_issuer("test@example.com");
        let cmd = CreateWorkerCommand::with_metadata(
            make_test_worker_spec(),
            ProviderId::new(),
            JobId::new(),
            "saga-456".to_string(),
            metadata,
        );

        // Custom metadata was provided, so issuer should be set
        assert_eq!(cmd.metadata.issuer.as_deref(), Some("test@example.com"));
    }

    #[tokio::test]
    async fn create_worker_handler_provisions_worker() {
        let provisioning = MockWorkerProvisioning;
        let handler = CreateWorkerHandler::new(provisioning);
        let cmd = CreateWorkerCommand::new(
            make_test_worker_spec(),
            ProviderId::new(),
            JobId::new(),
            "saga-789".to_string(),
        );

        let result = handler.handle(cmd).await;

        assert!(result.is_ok());
        let worker_result = result.unwrap();
        assert!(worker_result.worker_id.to_string().len() > 0);
    }

    #[tokio::test]
    async fn destroy_worker_command_idempotency_key() {
        let worker_id = WorkerId::new();
        let saga_id = "saga-abc".to_string();
        let cmd = DestroyWorkerCommand::new(worker_id.clone(), ProviderId::new(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains("saga-abc"));
        assert!(key.contains(&worker_id.to_string()));
        assert!(key.contains("destroy-worker"));
    }

    #[tokio::test]
    async fn destroy_worker_command_with_reason() {
        let worker_id = WorkerId::new();
        let cmd = DestroyWorkerCommand::with_reason(
            worker_id,
            ProviderId::new(),
            "saga-xyz".to_string(),
            "Saga compensation triggered",
        );

        assert_eq!(cmd.reason, Some("Saga compensation triggered".to_string()));
    }

    #[tokio::test]
    async fn destroy_worker_handler_destroys_worker() {
        let provisioning = MockWorkerProvisioning;
        let handler = DestroyWorkerHandler::new(provisioning);
        let cmd = DestroyWorkerCommand::new(
            WorkerId::new(),
            ProviderId::new(),
            "saga-destroy".to_string(),
        );

        let result = handler.handle(cmd).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ());
    }

    #[tokio::test]
    async fn create_worker_command_serializes() {
        let saga_id = "saga-serialize".to_string();
        let cmd = CreateWorkerCommand::new(
            make_test_worker_spec(),
            ProviderId::new(),
            JobId::new(),
            saga_id.clone(),
        );

        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: CreateWorkerCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(cmd.idempotency_key(), deserialized.idempotency_key());
        assert_eq!(cmd.saga_id, deserialized.saga_id);
        assert_eq!(cmd.provider_id, deserialized.provider_id);
    }
}
