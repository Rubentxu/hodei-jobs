//! Worker-related Commands for Saga Pattern
//!
//! Commands for worker lifecycle management, particularly for saga compensation.

use crate::shared_kernel::{ProviderId, WorkerId};
use serde::{Deserialize, Serialize};

// ============================================================================
// ReleaseWorkerCommand
// ============================================================================

/// Command to release a worker back to READY state.
///
/// This command is used primarily for saga compensation when a job assignment
/// fails or needs to be rolled back. It:
/// 1. Updates worker state from BUSY to READY
/// 2. Clears the current_job_id association
/// 3. Publishes WorkerReleased event
///
/// # Use Cases
/// - ExecutionSaga compensation (when AssignWorker step needs to be undone)
/// - Manual job cancellation
/// - Error recovery scenarios
///
/// # Idempotency
/// This command is idempotent - releasing an already READY worker is a no-op.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseWorkerCommand {
    /// Worker to release
    pub worker_id: WorkerId,
    /// Correlation ID for tracing (usually saga_id)
    pub correlation_id: String,
    /// Optional reason for release (for audit trail)
    pub reason: Option<String>,
}

impl ReleaseWorkerCommand {
    /// Create a new ReleaseWorkerCommand
    pub fn new(worker_id: WorkerId, correlation_id: String) -> Self {
        Self {
            worker_id,
            correlation_id,
            reason: None,
        }
    }

    /// Create a ReleaseWorkerCommand with a reason
    pub fn with_reason(worker_id: WorkerId, correlation_id: String, reason: String) -> Self {
        Self {
            worker_id,
            correlation_id,
            reason: Some(reason),
        }
    }

    /// Get the worker ID
    pub fn worker_id(&self) -> &WorkerId {
        &self.worker_id
    }

    /// Get the correlation ID
    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    /// Get the release reason
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

// ============================================================================
// MarkWorkerFailedCommand
// ============================================================================

/// Command to mark a worker as FAILED.
///
/// This command is used when a worker becomes unreachable, crashes, or
/// otherwise enters a failure state. It:
/// 1. Updates worker state to FAILED
/// 2. Publishes WorkerFailed event
/// 3. Triggers cleanup of associated job (if any)
///
/// # Use Cases
/// - Heartbeat timeout exceeded
/// - Worker process crashed
/// - Network partition detected
/// - Provider infrastructure failure
///
/// # Idempotency
/// This command is idempotent - marking an already FAILED worker is a no-op.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkWorkerFailedCommand {
    /// Worker to mark as failed
    pub worker_id: WorkerId,
    /// Provider ID (for cleanup)
    pub provider_id: ProviderId,
    /// Correlation ID for tracing
    pub correlation_id: String,
    /// Reason for failure (required for audit trail)
    pub reason: String,
    /// Whether to trigger automatic job reassignment
    pub reassign_job: bool,
}

impl MarkWorkerFailedCommand {
    /// Create a new MarkWorkerFailedCommand
    pub fn new(
        worker_id: WorkerId,
        provider_id: ProviderId,
        correlation_id: String,
        reason: String,
    ) -> Self {
        Self {
            worker_id,
            provider_id,
            correlation_id,
            reason,
            reassign_job: true, // Default to auto-reassignment
        }
    }

    /// Create a command without triggering job reassignment
    pub fn without_reassignment(mut self) -> Self {
        self.reassign_job = false;
        self
    }

    /// Get the worker ID
    pub fn worker_id(&self) -> &WorkerId {
        &self.worker_id
    }

    /// Get the provider ID
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Get the correlation ID
    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    /// Get the failure reason
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Should trigger job reassignment
    pub fn should_reassign_job(&self) -> bool {
        self.reassign_job
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_release_worker_command_creation() {
        let worker_id = WorkerId::new();
        let correlation_id = "saga-123".to_string();

        let cmd = ReleaseWorkerCommand::new(worker_id.clone(), correlation_id.clone());

        assert_eq!(cmd.worker_id(), &worker_id);
        assert_eq!(cmd.correlation_id(), correlation_id);
        assert!(cmd.reason().is_none());
    }

    #[test]
    fn test_release_worker_command_with_reason() {
        let worker_id = WorkerId::new();
        let correlation_id = "saga-456".to_string();
        let reason = "Saga compensation".to_string();

        let cmd = ReleaseWorkerCommand::with_reason(
            worker_id.clone(),
            correlation_id.clone(),
            reason.clone(),
        );

        assert_eq!(cmd.worker_id(), &worker_id);
        assert_eq!(cmd.correlation_id(), correlation_id);
        assert_eq!(cmd.reason(), Some(reason.as_str()));
    }

    #[test]
    fn test_mark_worker_failed_command_creation() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let correlation_id = "heartbeat-timeout".to_string();
        let reason = "Heartbeat timeout exceeded".to_string();

        let cmd = MarkWorkerFailedCommand::new(
            worker_id.clone(),
            provider_id.clone(),
            correlation_id.clone(),
            reason.clone(),
        );

        assert_eq!(cmd.worker_id(), &worker_id);
        assert_eq!(cmd.provider_id(), &provider_id);
        assert_eq!(cmd.correlation_id(), correlation_id);
        assert_eq!(cmd.reason(), reason);
        assert!(cmd.should_reassign_job());
    }

    #[test]
    fn test_mark_worker_failed_without_reassignment() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let correlation_id = "manual-shutdown".to_string();
        let reason = "Manual shutdown".to_string();

        let cmd = MarkWorkerFailedCommand::new(
            worker_id.clone(),
            provider_id.clone(),
            correlation_id,
            reason,
        )
        .without_reassignment();

        assert!(!cmd.should_reassign_job());
    }
}
