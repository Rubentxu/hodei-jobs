//! Saga Error Types - EPIC-46
//!
//! Strongly typed error enums for each saga type as specified in EPIC-46.
//! This provides better error handling and debugging than generic SagaError.

use crate::saga::{SagaId, SagaType};
use crate::shared_kernel::{JobId, ProviderId, WorkerId};

// ============================================================================
// Core Saga Errors
// ============================================================================

/// Core errors that can occur during saga execution (EPIC-46 Section 5.3)
#[derive(Debug, thiserror::Error)]
pub enum SagaCoreError {
    #[error("Saga execution failed: {message}")]
    ExecutionFailed { message: String, saga_id: SagaId },

    #[error("Saga compensation failed: {message}")]
    CompensationFailed { message: String, saga_id: SagaId },

    #[error("Saga not found: {saga_id}")]
    NotFound { saga_id: SagaId },

    #[error("Invalid saga state transition: from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Saga {saga_id} timed out after {duration:?}")]
    Timeout {
        saga_id: SagaId,
        duration: std::time::Duration,
    },

    #[error("Saga {saga_id} was cancelled")]
    Cancelled { saga_id: SagaId },

    #[error(
        "Optimistic lock conflict for saga {saga_id}: expected version {expected}, found {actual}"
    )]
    OptimisticLockConflict {
        saga_id: SagaId,
        expected: u64,
        actual: u64,
    },

    #[error("Persistence error: {message}")]
    PersistenceError { message: String },
}

// ============================================================================
// Execution Saga Errors (EPIC-46 Section 5.3)
// ============================================================================

/// Errors specific to ExecutionSaga (EPIC-46 Section 5.3)
#[derive(Debug, thiserror::Error)]
pub enum ExecutionSagaError {
    #[error("No available workers for job {job_id}")]
    NoAvailableWorkers { job_id: JobId },

    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: JobId },

    #[error("Failed to dispatch job {job_id} to worker {worker_id}: {message}")]
    DispatchFailed {
        job_id: JobId,
        worker_id: WorkerId,
        message: String,
    },

    #[error("Job assignment failed for job {job_id}: {message}")]
    WorkerAssignmentFailed { job_id: JobId, message: String },

    #[error("State update failed for job {job_id}: {message}")]
    StateUpdateFailed { job_id: JobId, message: String },

    #[error("Compensation failed for job {job_id}: {message}")]
    CompensationFailed { job_id: JobId, message: String },

    #[error("Job execution timed out after {duration:?}")]
    ExecutionTimeout {
        job_id: JobId,
        duration: std::time::Duration,
    },

    #[error("Worker {worker_id} disconnected during execution")]
    WorkerDisconnected { worker_id: WorkerId },

    #[error("Job {job_id} result invalid: {reason}")]
    InvalidResult { job_id: JobId, reason: String },
}

// ============================================================================
// Provisioning Saga Errors
// ============================================================================

/// Errors specific to ProvisioningSaga
#[derive(Debug, thiserror::Error)]
pub enum ProvisioningSagaError {
    #[error("Provider {provider_id} not found")]
    ProviderNotFound { provider_id: ProviderId },

    #[error("Provider {provider_id} capacity exceeded")]
    ProviderCapacityExceeded { provider_id: ProviderId },

    #[error("Worker provisioning failed for provider {provider_id}: {message}")]
    ProvisioningFailed {
        provider_id: ProviderId,
        message: String,
    },

    #[error("Worker registration failed: {message}")]
    RegistrationFailed { message: String },

    #[error("Worker {worker_id} already exists")]
    WorkerAlreadyExists { worker_id: WorkerId },

    #[error("Bootstrap token generation failed")]
    BootstrapTokenGenerationFailed,

    #[error("Infrastructure cleanup failed: {message}")]
    CleanupFailed { message: String },
}

// ============================================================================
// Recovery Saga Errors
// ============================================================================

/// Errors specific to RecoverySaga
#[derive(Debug, thiserror::Error)]
pub enum RecoverySagaError {
    #[error("Worker {worker_id} not found for recovery")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Job {job_id} not found for recovery")]
    JobNotFound { job_id: JobId },

    #[error("Recovery failed for job {job_id}: {reason}")]
    RecoveryFailed { job_id: JobId, reason: String },

    #[error("Cannot recover job {job_id}: {reason}")]
    RecoveryNotPossible { job_id: JobId, reason: String },

    #[error("New worker provisioning failed during recovery: {message}")]
    NewWorkerProvisioningFailed { message: String },

    #[error("Job transfer failed: {message}")]
    TransferFailed { message: String },

    #[error("Old worker termination failed: {message}")]
    TerminationFailed { message: String },
}

// ============================================================================
// Cancellation Saga Errors
// ============================================================================

/// Errors specific to CancellationSaga
#[derive(Debug, thiserror::Error)]
pub enum CancellationSagaError {
    #[error("Job {job_id} cannot be cancelled: {reason}")]
    JobNotCancellable { job_id: JobId, reason: String },

    #[error("Job {job_id} not found for cancellation")]
    JobNotFound { job_id: JobId },

    #[error("Worker notification failed for job {job_id}: {message}")]
    WorkerNotificationFailed { job_id: JobId, message: String },

    #[error("Job state update failed for {job_id}: {message}")]
    StateUpdateFailed { job_id: JobId, message: String },

    #[error("Worker release failed for job {job_id}: {message}")]
    WorkerReleaseFailed { job_id: JobId, message: String },

    #[error("Cancellation event publishing failed: {message}")]
    EventPublishingFailed { message: String },
}

// ============================================================================
// Timeout Saga Errors
// ============================================================================

/// Errors specific to TimeoutSaga
#[derive(Debug, thiserror::Error)]
pub enum TimeoutSagaError {
    #[error("Job {job_id} not found for timeout handling")]
    JobNotFound { job_id: JobId },

    #[error("Timeout detection failed for job {job_id}: {message}")]
    TimeoutDetectionFailed { job_id: JobId, message: String },

    #[error("Cannot initiate timeout handling for job {job_id}: {reason}")]
    TimeoutHandlingNotPossible { job_id: JobId, reason: String },

    #[error("Worker termination failed for timed out job {job_id}: {message}")]
    WorkerTerminationFailed { job_id: JobId, message: String },

    #[error("Job state update to failed failed for {job_id}: {message}")]
    StateUpdateFailed { job_id: JobId, message: String },
}

// ============================================================================
// Cleanup Saga Errors
// ============================================================================

/// Errors specific to CleanupSaga
#[derive(Debug, thiserror::Error)]
pub enum CleanupSagaError {
    #[error("Failed to identify orphaned jobs: {message}")]
    IdentifyOrphanedJobsFailed { message: String },

    #[error("Failed to identify unhealthy workers: {message}")]
    IdentifyUnhealthyWorkersFailed { message: String },

    #[error("Failed to reset orphaned job {job_id}: {message}")]
    ResetOrphanedJobFailed { job_id: JobId, message: String },

    #[error("Cleanup metrics publishing failed: {message}")]
    MetricsPublishingFailed { message: String },

    #[error("Partial cleanup completed with {failed_count} failures")]
    PartialCleanup { failed_count: usize },
}

// ============================================================================
// Convenience Traits
// ============================================================================

/// Convert saga-specific errors to SagaCoreError
pub trait IntoSagaCoreError {
    fn into_core_error(self, saga_id: SagaId, saga_type: SagaType) -> SagaCoreError;
}

impl IntoSagaCoreError for ExecutionSagaError {
    fn into_core_error(self, saga_id: SagaId, _saga_type: SagaType) -> SagaCoreError {
        match self {
            ExecutionSagaError::NoAvailableWorkers { job_id } => SagaCoreError::ExecutionFailed {
                message: format!("No available workers for job {}", job_id),
                saga_id,
            },
            ExecutionSagaError::JobNotFound { job_id } => SagaCoreError::NotFound { saga_id },
            ExecutionSagaError::DispatchFailed {
                job_id,
                worker_id,
                message,
            } => SagaCoreError::ExecutionFailed {
                message: format!(
                    "Dispatch failed for job {} to worker {}: {}",
                    job_id, worker_id, message
                ),
                saga_id,
            },
            ExecutionSagaError::WorkerAssignmentFailed { job_id, message } => {
                SagaCoreError::ExecutionFailed {
                    message: format!("Worker assignment failed for job {}: {}", job_id, message),
                    saga_id,
                }
            }
            ExecutionSagaError::StateUpdateFailed { job_id, message } => {
                SagaCoreError::ExecutionFailed {
                    message: format!("State update failed for job {}: {}", job_id, message),
                    saga_id,
                }
            }
            ExecutionSagaError::CompensationFailed { job_id, message } => {
                SagaCoreError::CompensationFailed {
                    message: format!("Compensation failed for job {}: {}", job_id, message),
                    saga_id,
                }
            }
            ExecutionSagaError::ExecutionTimeout { job_id, duration } => {
                SagaCoreError::Timeout { saga_id, duration }
            }
            ExecutionSagaError::WorkerDisconnected { worker_id } => {
                SagaCoreError::ExecutionFailed {
                    message: format!("Worker {} disconnected during execution", worker_id),
                    saga_id,
                }
            }
            ExecutionSagaError::InvalidResult { job_id, reason } => {
                SagaCoreError::ExecutionFailed {
                    message: format!("Invalid result for job {}: {}", job_id, reason),
                    saga_id,
                }
            }
        }
    }
}

impl IntoSagaCoreError for ProvisioningSagaError {
    fn into_core_error(self, saga_id: SagaId, _saga_type: SagaType) -> SagaCoreError {
        match self {
            ProvisioningSagaError::ProviderNotFound { provider_id } => {
                SagaCoreError::NotFound { saga_id }
            }
            ProvisioningSagaError::ProviderCapacityExceeded { provider_id } => {
                SagaCoreError::ExecutionFailed {
                    message: format!("Provider {} capacity exceeded", provider_id),
                    saga_id,
                }
            }
            ProvisioningSagaError::ProvisioningFailed {
                provider_id,
                message,
            } => SagaCoreError::ExecutionFailed {
                message: format!(
                    "Provisioning failed for provider {}: {}",
                    provider_id, message
                ),
                saga_id,
            },
            ProvisioningSagaError::RegistrationFailed { message } => {
                SagaCoreError::ExecutionFailed {
                    message: format!("Worker registration failed: {}", message),
                    saga_id,
                }
            }
            ProvisioningSagaError::WorkerAlreadyExists { worker_id } => {
                SagaCoreError::ExecutionFailed {
                    message: format!("Worker {} already exists", worker_id),
                    saga_id,
                }
            }
            ProvisioningSagaError::BootstrapTokenGenerationFailed => {
                SagaCoreError::ExecutionFailed {
                    message: "Bootstrap token generation failed".to_string(),
                    saga_id,
                }
            }
            ProvisioningSagaError::CleanupFailed { message } => SagaCoreError::CompensationFailed {
                message: format!("Infrastructure cleanup failed: {}", message),
                saga_id,
            },
        }
    }
}

// ============================================================================
// From implementations for compatibility
// ============================================================================

impl From<SagaCoreError> for crate::shared_kernel::DomainError {
    fn from(error: SagaCoreError) -> Self {
        match error {
            SagaCoreError::ExecutionFailed { message, .. } => {
                crate::shared_kernel::DomainError::InfrastructureError { message }
            }
            SagaCoreError::CompensationFailed { message, .. } => {
                crate::shared_kernel::DomainError::InfrastructureError { message }
            }
            SagaCoreError::NotFound { .. } => {
                crate::shared_kernel::DomainError::InfrastructureError {
                    message: "Saga not found".to_string(),
                }
            }
            SagaCoreError::InvalidStateTransition { from, to } => {
                crate::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Invalid state transition from {} to {}", from, to),
                }
            }
            SagaCoreError::Timeout { duration, .. } => {
                crate::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Saga timed out after {:?}", duration),
                }
            }
            SagaCoreError::Cancelled { .. } => {
                crate::shared_kernel::DomainError::InfrastructureError {
                    message: "Saga was cancelled".to_string(),
                }
            }
            SagaCoreError::OptimisticLockConflict {
                saga_id,
                expected,
                actual,
            } => crate::shared_kernel::DomainError::InfrastructureError {
                message: format!(
                    "Optimistic lock conflict for saga {}: expected {}, found {}",
                    saga_id, expected, actual
                ),
            },
            SagaCoreError::PersistenceError { message } => {
                crate::shared_kernel::DomainError::InfrastructureError { message }
            }
        }
    }
}
