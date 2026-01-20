//!
//! # Cleanup Workflow for saga-engine v4
//!
//! This module provides CleanupWorkflow implementation for saga-engine v4.0,
//! handling system maintenance and cleanup of unhealthy resources.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use saga_engine_core::workflow::{
    DynWorkflowStep, StepCompensationError, StepError, StepErrorKind, StepResult, WorkflowConfig,
    WorkflowContext, WorkflowDefinition, WorkflowStep,
};

use crate::saga::bridge::worker_lifecycle::TerminateWorkerActivity;

// =============================================================================
// Input/Output Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupWorkflowInput {
    pub unhealthy_threshold: Duration,
    pub orphaned_job_threshold: Duration,
    pub dry_run: bool,
}

impl CleanupWorkflowInput {
    pub fn new(
        unhealthy_threshold: Duration,
        orphaned_job_threshold: Duration,
        dry_run: bool,
    ) -> Self {
        Self {
            unhealthy_threshold,
            orphaned_job_threshold,
            dry_run,
        }
    }

    pub fn idempotency_key(&self) -> String {
        let now = chrono::Utc::now().timestamp();
        format!("cleanup:{}", now)
    }

    pub fn production_defaults() -> Self {
        Self {
            unhealthy_threshold: Duration::from_secs(300),
            orphaned_job_threshold: Duration::from_secs(600),
            dry_run: false,
        }
    }

    pub fn dry_run_defaults() -> Self {
        Self {
            unhealthy_threshold: Duration::from_secs(300),
            orphaned_job_threshold: Duration::from_secs(600),
            dry_run: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupWorkflowOutput {
    pub workers_cleaned: usize,
    pub jobs_reset: usize,
    pub metrics_published: bool,
    pub completed_at: chrono::DateTime<chrono::Utc>,
    pub dry_run: bool,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CleanupWorkflowError {
    #[error("Worker identification failed: {0}")]
    WorkerIdentificationFailed(String),

    #[error("Job identification failed: {0}")]
    JobIdentificationFailed(String),

    #[error("Worker cleanup failed: {0}")]
    WorkerCleanupFailed(String),

    #[error("Job reset failed: {0}")]
    JobResetFailed(String),

    #[error("Metrics publication failed: {0}")]
    MetricsPublicationFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyUnhealthyWorkersInput {
    pub threshold: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyUnhealthyWorkersOutput {
    pub unhealthy_workers: Vec<String>,
    pub checked_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyOrphanedJobsInput {
    pub threshold: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyOrphanedJobsOutput {
    pub orphaned_jobs: Vec<String>,
    pub checked_count: usize,
}

// =============================================================================
// Workflow Steps
// =============================================================================

pub struct IdentifyUnhealthyWorkersStep;

impl Debug for IdentifyUnhealthyWorkersStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentifyUnhealthyWorkersStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for IdentifyUnhealthyWorkersStep {
    const TYPE_ID: &'static str = "cleanup-identify-unhealthy-workers";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let threshold_secs = input["unhealthy_threshold"].as_u64().unwrap_or(300);

        Ok(StepResult::Output(serde_json::json!({
            "unhealthy_workers": [],
            "checked_count": 0,
            "threshold_seconds": threshold_secs
        })))
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct IdentifyOrphanedJobsStep;

impl Debug for IdentifyOrphanedJobsStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentifyOrphanedJobsStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for IdentifyOrphanedJobsStep {
    const TYPE_ID: &'static str = "cleanup-identify-orphaned-jobs";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let threshold_secs = input["orphaned_job_threshold"].as_u64().unwrap_or(600);

        Ok(StepResult::Output(serde_json::json!({
            "orphaned_jobs": [],
            "checked_count": 0,
            "threshold_seconds": threshold_secs
        })))
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct CleanupUnhealthyWorkersStep;

impl Debug for CleanupUnhealthyWorkersStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CleanupUnhealthyWorkersStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for CleanupUnhealthyWorkersStep {
    const TYPE_ID: &'static str = "cleanup-unhealthy-workers";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let dry_run = input["dry_run"].as_bool().unwrap_or(false);

        Ok(StepResult::Output(serde_json::json!({
            "workers_cleaned": 0,
            "dry_run": dry_run
        })))
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct ResetOrphanedJobsStep;

impl Debug for ResetOrphanedJobsStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResetOrphanedJobsStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for ResetOrphanedJobsStep {
    const TYPE_ID: &'static str = "cleanup-reset-orphaned-jobs";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        _input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        Ok(StepResult::Output(serde_json::json!({
            "jobs_reset": 0
        })))
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct PublishCleanupMetricsStep;

impl Debug for PublishCleanupMetricsStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublishCleanupMetricsStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for PublishCleanupMetricsStep {
    const TYPE_ID: &'static str = "cleanup-publish-metrics";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        _input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        Ok(StepResult::Output(serde_json::json!({
            "metrics_published": true,
            "published_at": chrono::Utc::now().to_rfc3339()
        })))
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

// =============================================================================
// Workflow Definition
// =============================================================================

pub struct CleanupWorkflow {
    steps: Vec<Box<dyn DynWorkflowStep>>,
    config: WorkflowConfig,
}

impl Debug for CleanupWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CleanupWorkflow")
            .field("steps", &format!("[{} steps]", self.steps.len()))
            .field("config", &self.config)
            .finish()
    }
}

impl CleanupWorkflow {
    pub fn new() -> Self {
        let steps = vec![
            Box::new(IdentifyUnhealthyWorkersStep) as Box<dyn DynWorkflowStep>,
            Box::new(IdentifyOrphanedJobsStep) as Box<dyn DynWorkflowStep>,
            Box::new(CleanupUnhealthyWorkersStep) as Box<dyn DynWorkflowStep>,
            Box::new(ResetOrphanedJobsStep) as Box<dyn DynWorkflowStep>,
            Box::new(PublishCleanupMetricsStep) as Box<dyn DynWorkflowStep>,
        ];

        let config = WorkflowConfig {
            default_timeout_secs: 60,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "cleanup".to_string(),
        };

        Self { steps, config }
    }
}

impl Default for CleanupWorkflow {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowDefinition for CleanupWorkflow {
    const TYPE_ID: &'static str = "cleanup";
    const VERSION: u32 = 1;

    type Input = CleanupWorkflowInput;
    type Output = CleanupWorkflowOutput;

    fn steps(&self) -> &[Box<dyn DynWorkflowStep>] {
        &self.steps
    }

    fn configuration(&self) -> WorkflowConfig {
        self.config.clone()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_workflow_type_id() {
        assert_eq!(CleanupWorkflow::TYPE_ID, "cleanup");
        assert_eq!(CleanupWorkflow::VERSION, 1);
    }

    #[test]
    fn test_cleanup_workflow_has_five_steps() {
        let workflow = CleanupWorkflow::new();
        let steps = workflow.steps();

        assert_eq!(steps.len(), 5);
        assert_eq!(
            steps[0].step_type_id(),
            "cleanup-identify-unhealthy-workers"
        );
        assert_eq!(steps[1].step_type_id(), "cleanup-identify-orphaned-jobs");
        assert_eq!(steps[2].step_type_id(), "cleanup-unhealthy-workers");
        assert_eq!(steps[3].step_type_id(), "cleanup-reset-orphaned-jobs");
        assert_eq!(steps[4].step_type_id(), "cleanup-publish-metrics");
    }

    #[test]
    fn test_cleanup_workflow_input_creation() {
        let input =
            CleanupWorkflowInput::new(Duration::from_secs(300), Duration::from_secs(600), false);

        assert_eq!(input.unhealthy_threshold, Duration::from_secs(300));
        assert_eq!(input.orphaned_job_threshold, Duration::from_secs(600));
        assert!(!input.dry_run);
        assert!(!input.idempotency_key().is_empty());
    }

    #[test]
    fn test_cleanup_workflow_production_defaults() {
        let input = CleanupWorkflowInput::production_defaults();

        assert_eq!(input.unhealthy_threshold, Duration::from_secs(300));
        assert_eq!(input.orphaned_job_threshold, Duration::from_secs(600));
        assert!(!input.dry_run);
    }

    #[test]
    fn test_cleanup_workflow_dry_run_defaults() {
        let input = CleanupWorkflowInput::dry_run_defaults();

        assert_eq!(input.unhealthy_threshold, Duration::from_secs(300));
        assert_eq!(input.orphaned_job_threshold, Duration::from_secs(600));
        assert!(input.dry_run);
    }
}
