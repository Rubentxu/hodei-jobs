//! Workflow implementations for saga-engine v4.0 (EPIC-94)
//!
//! Provides workflow definitions for the saga-engine v4.0 migration,
//! implementing the WorkflowDefinition trait for each saga type.

/// Provisioning workflow for worker on-demand provisioning
pub mod provisioning;

/// Execution workflow for job execution
pub mod execution;
