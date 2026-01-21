//! Workflow implementations for saga-engine v4.0 (EPIC-94)
//!
//! Provides workflow definitions for saga-engine v4.0 migration,
//! implementing WorkflowDefinition trait for each saga type.
//!
//! ## Workflow Types
//!
//! ### Provisioning Workflow
//!
//! Provisioning workflow for worker on-demand provisioning.
//!
//! ### Execution Workflow
//!
//! Execution workflow for job execution.
//!
//! ### Recovery Workflow
//!
//! Recovery workflow for worker failure recovery.
//!
//! ### Cancellation Workflow
//!
//! Cancellation workflow for job cancellation.
//!
//! ### Timeout Workflow
//!
//! Timeout workflow for job timeout handling.
//!
//! ### Cleanup Workflow
//!
//! Cleanup workflow for system maintenance.

/// Provisioning workflow for worker on-demand provisioning
pub mod provisioning;

/// Provisioning workflow v2 (DurableWorkflow - Workflow-as-Code)
pub mod provisioning_durable;

/// Execution workflow for job execution
pub mod execution;

/// Execution workflow v2 (DurableWorkflow - Workflow-as-Code)
pub mod execution_durable;

/// Recovery workflow for worker failure recovery
pub mod recovery;

/// Recovery workflow v2 (DurableWorkflow - Workflow-as-Code)
pub mod recovery_durable;

/// Cancellation workflow for job cancellation
pub mod cancellation;

/// Timeout workflow for job timeout handling
pub mod timeout;

/// Cleanup workflow for system maintenance
pub mod cleanup;

// Re-export common types
pub use execution::{CollectedResult, JobExecutionPort, JobResultData};
pub use execution_durable::{
    ExecutionWorkflow, ExecutionWorkflowError, ExecutionWorkflowInput, ExecutionWorkflowOutput,
};
