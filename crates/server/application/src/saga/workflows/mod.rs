//! Workflow implementations for saga-engine v4.0 (EPIC-94)
//!
//! Provides workflow definitions using the DurableWorkflow trait
//! for the Workflow-as-Code pattern.
//!
//! ## Workflow Types
//!
//! ### Provisioning Workflow (v4)
//!
//! Provisioning workflow for worker on-demand provisioning.
//!
//! ### Execution Workflow (v4)
//!
//! Execution workflow for job execution.
//!
//! ### Recovery Workflow (v4)
//!
//! Recovery workflow for worker failure recovery.
//!
//! ### Cancellation Workflow (v4)
//!
//! Cancellation workflow for job cancellation.
//!
//! ### Timeout Workflow (v4)
//!
//! Timeout workflow for job timeout handling.
//!
//! ### Cleanup Workflow (v4)
//!
//! Cleanup workflow for system maintenance.

/// Provisioning workflow v4 (DurableWorkflow - Workflow-as-Code)
pub mod provisioning_durable;

/// Execution workflow v4 (DurableWorkflow - Workflow-as-Code)
pub mod execution_durable;

/// Recovery workflow v4 (DurableWorkflow - Workflow-as-Code)
pub mod recovery_durable;

/// Cancellation workflow v4 (DurableWorkflow - Workflow-as-Code)
pub mod cancellation_durable;

/// Timeout workflow v4 (DurableWorkflow - Workflow-as-Code)
pub mod timeout_durable;

/// Cleanup workflow v4 (DurableWorkflow - Workflow-as-Code)
pub mod cleanup_durable;

// Re-export common types from DurableWorkflow implementations
pub use cancellation_durable::{
    CancellationDurableWorkflow, CancellationWorkflowError, CancellationWorkflowInput,
    CancellationWorkflowOutput,
};
pub use cleanup_durable::{
    CleanupDurableWorkflow, CleanupWorkflowError, CleanupWorkflowInput, CleanupWorkflowOutput,
};
pub use execution_durable::{
    CollectedResult, ExecutionWorkflow, ExecutionWorkflowError, ExecutionWorkflowInput,
    ExecutionWorkflowOutput, JobExecutionPort, JobResultData,
};
pub use provisioning_durable::{
    ProvisioningError, ProvisioningInput, ProvisioningOutput, ProvisioningWorkflow, WorkerSpecData,
};
pub use recovery_durable::{RecoveryError, RecoveryInput, RecoveryOutput, RecoveryWorkflow};
pub use timeout_durable::{
    TimeoutDurableWorkflow, TimeoutWorkflowError, TimeoutWorkflowInput, TimeoutWorkflowOutput,
};
