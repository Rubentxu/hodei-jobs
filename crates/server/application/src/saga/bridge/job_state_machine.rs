//!
//! # Job State Machine Activity
//!
//! Activities for job state transitions compatible with the saga-engine.
//! These activities wrap job management operations as saga-compatible steps.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

use hodei_server_domain::jobs::{Job, JobRepository, JobSpec};
use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, WorkerId};
use hodei_shared::states::JobState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobInput {
    pub spec: JobSpec,
    pub saga_id: Option<String>,
}

impl CreateJobInput {
    pub fn new(spec: JobSpec, saga_id: Option<String>) -> Self {
        Self { spec, saga_id }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobOutput {
    pub job_id: JobId,
    pub state: JobState,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl CreateJobOutput {
    pub fn success(job_id: JobId, state: JobState) -> Self {
        Self {
            job_id,
            state,
            created_at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStateTransitionInput {
    pub job_id: JobId,
    pub target_state: JobState,
    pub reason: Option<String>,
}

impl JobStateTransitionInput {
    pub fn new(job_id: JobId, target_state: JobState, reason: Option<String>) -> Self {
        Self {
            job_id,
            target_state,
            reason,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStateTransitionOutput {
    pub job_id: JobId,
    pub previous_state: JobState,
    pub new_state: JobState,
    pub transitioned_at: chrono::DateTime<chrono::Utc>,
}

impl JobStateTransitionOutput {
    pub fn success(job_id: JobId, previous_state: JobState, new_state: JobState) -> Self {
        Self {
            job_id,
            previous_state,
            new_state,
            transitioned_at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignJobInput {
    pub job_id: JobId,
    pub worker_id: WorkerId,
    pub provider_id: ProviderId,
}

impl AssignJobInput {
    pub fn new(job_id: JobId, worker_id: WorkerId, provider_id: ProviderId) -> Self {
        Self {
            job_id,
            worker_id,
            provider_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignJobOutput {
    pub job_id: JobId,
    pub worker_id: WorkerId,
    pub provider_id: ProviderId,
    pub state: JobState,
    pub assigned_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteJobInput {
    pub job_id: JobId,
    pub exit_code: i32,
    pub output: String,
    pub error_output: String,
}

impl CompleteJobInput {
    pub fn success(job_id: JobId, exit_code: i32, output: String, error_output: String) -> Self {
        Self {
            job_id,
            exit_code,
            output,
            error_output,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteJobOutput {
    pub job_id: JobId,
    pub final_state: JobState,
    pub exit_code: i32,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailJobInput {
    pub job_id: JobId,
    pub reason: String,
    pub exit_code: i32,
    pub error_output: String,
}

impl FailJobInput {
    pub fn new(job_id: JobId, reason: String, exit_code: i32, error_output: String) -> Self {
        Self {
            job_id,
            reason,
            exit_code,
            error_output,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailJobOutput {
    pub job_id: JobId,
    pub final_state: JobState,
    pub reason: String,
    pub failed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelJobInput {
    pub job_id: JobId,
    pub reason: String,
}

impl CancelJobInput {
    pub fn new(job_id: JobId, reason: String) -> Self {
        Self { job_id, reason }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelJobOutput {
    pub job_id: JobId,
    pub final_state: JobState,
    pub cancelled_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum JobStateMachineError {
    #[error("Job not found")]
    JobNotFound,
    #[error("Invalid state transition")]
    InvalidTransition,
    #[error("Job is in terminal state")]
    TerminalState,
    #[error("Failed to update job state")]
    StateUpdateFailed,
    #[error("Failed to create job")]
    CreationFailed,
    #[error("Failed to assign job")]
    AssignmentFailed,
    #[error("Failed to complete job")]
    CompletionFailed,
}

pub struct CreateJobActivity {
    repository: Arc<dyn JobRepository + Send + Sync>,
}

impl CreateJobActivity {
    pub fn new(repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl crate::saga::bridge::command_bus::Activity for CreateJobActivity {
    type Input = CreateJobInput;
    type Output = CreateJobOutput;
    type Error = JobStateMachineError;

    fn activity_type_id(&self) -> &'static str {
        "create-job"
    }

    fn task_queue(&self) -> Option<&str> {
        Some("job-management")
    }

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job_id = JobId::new();
        let job = Job::new(job_id.clone(), "job".to_string(), input.spec);

        self.repository
            .save(&job)
            .await
            .map_err(|_| JobStateMachineError::CreationFailed)?;

        Ok(CreateJobOutput::success(job_id, JobState::Pending))
    }
}

pub struct TransitionJobStateActivity {
    repository: Arc<dyn JobRepository + Send + Sync>,
}

impl TransitionJobStateActivity {
    pub fn new(repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl crate::saga::bridge::command_bus::Activity for TransitionJobStateActivity {
    type Input = JobStateTransitionInput;
    type Output = JobStateTransitionOutput;
    type Error = JobStateMachineError;

    fn activity_type_id(&self) -> &'static str {
        "transition-job-state"
    }

    fn task_queue(&self) -> Option<&str> {
        Some("job-management")
    }

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let target_state = input.target_state.clone();
        let job = self
            .repository
            .find_by_id(&input.job_id)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?
            .ok_or(JobStateMachineError::JobNotFound)?;

        if job.state().is_terminal() {
            return Err(JobStateMachineError::TerminalState);
        }

        if !job.state().can_transition_to(&target_state) {
            return Err(JobStateMachineError::InvalidTransition);
        }

        let previous_state = job.state().clone();
        let target = target_state.clone();
        self.repository
            .update_state(&input.job_id, target)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?;

        Ok(JobStateTransitionOutput::success(
            input.job_id,
            previous_state,
            target_state,
        ))
    }
}

pub struct AssignJobActivity {
    repository: Arc<dyn JobRepository + Send + Sync>,
}

impl AssignJobActivity {
    pub fn new(repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl crate::saga::bridge::command_bus::Activity for AssignJobActivity {
    type Input = AssignJobInput;
    type Output = AssignJobOutput;
    type Error = JobStateMachineError;

    fn activity_type_id(&self) -> &'static str {
        "assign-job"
    }

    fn task_queue(&self) -> Option<&str> {
        Some("job-management")
    }

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let worker_id = input.worker_id.clone();
        let provider_id = input.provider_id.clone();
        let job = self
            .repository
            .find_by_id(&input.job_id)
            .await
            .map_err(|_| JobStateMachineError::AssignmentFailed)?
            .ok_or(JobStateMachineError::JobNotFound)?;

        if !job.state().can_transition_to(&JobState::Assigned) {
            return Err(JobStateMachineError::InvalidTransition);
        }

        self.repository
            .update_state(&input.job_id, JobState::Assigned)
            .await
            .map_err(|_| JobStateMachineError::AssignmentFailed)?;

        Ok(AssignJobOutput {
            job_id: input.job_id,
            worker_id,
            provider_id,
            state: JobState::Assigned,
            assigned_at: chrono::Utc::now(),
        })
    }
}

pub struct StartJobActivity {
    repository: Arc<dyn JobRepository + Send + Sync>,
}

impl StartJobActivity {
    pub fn new(repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl crate::saga::bridge::command_bus::Activity for StartJobActivity {
    type Input = JobId;
    type Output = JobState;
    type Error = JobStateMachineError;

    fn activity_type_id(&self) -> &'static str {
        "start-job"
    }

    fn task_queue(&self) -> Option<&str> {
        Some("job-management")
    }

    async fn execute(&self, job_id: Self::Input) -> Result<Self::Output, Self::Error> {
        let job = self
            .repository
            .find_by_id(&job_id)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?
            .ok_or(JobStateMachineError::JobNotFound)?;

        if !job.state().can_transition_to(&JobState::Running) {
            return Err(JobStateMachineError::InvalidTransition);
        }

        self.repository
            .update_state(&job_id, JobState::Running)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?;

        Ok(JobState::Running)
    }
}

pub struct CompleteJobActivity {
    repository: Arc<dyn JobRepository + Send + Sync>,
}

impl CompleteJobActivity {
    pub fn new(repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl crate::saga::bridge::command_bus::Activity for CompleteJobActivity {
    type Input = CompleteJobInput;
    type Output = CompleteJobOutput;
    type Error = JobStateMachineError;

    fn activity_type_id(&self) -> &'static str {
        "complete-job"
    }

    fn task_queue(&self) -> Option<&str> {
        Some("job-management")
    }

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job = self
            .repository
            .find_by_id(&input.job_id)
            .await
            .map_err(|_| JobStateMachineError::CompletionFailed)?
            .ok_or(JobStateMachineError::JobNotFound)?;

        if !job.state().can_transition_to(&JobState::Succeeded) {
            return Err(JobStateMachineError::InvalidTransition);
        }

        self.repository
            .update_state(&input.job_id, JobState::Succeeded)
            .await
            .map_err(|_| JobStateMachineError::CompletionFailed)?;

        Ok(CompleteJobOutput {
            job_id: input.job_id,
            final_state: JobState::Succeeded,
            exit_code: input.exit_code,
            completed_at: chrono::Utc::now(),
        })
    }
}

pub struct FailJobActivity {
    repository: Arc<dyn JobRepository + Send + Sync>,
}

impl FailJobActivity {
    pub fn new(repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl crate::saga::bridge::command_bus::Activity for FailJobActivity {
    type Input = FailJobInput;
    type Output = FailJobOutput;
    type Error = JobStateMachineError;

    fn activity_type_id(&self) -> &'static str {
        "fail-job"
    }

    fn task_queue(&self) -> Option<&str> {
        Some("job-management")
    }

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job = self
            .repository
            .find_by_id(&input.job_id)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?
            .ok_or(JobStateMachineError::JobNotFound)?;

        if !job.state().can_transition_to(&JobState::Failed) {
            return Err(JobStateMachineError::InvalidTransition);
        }

        self.repository
            .update_state(&input.job_id, JobState::Failed)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?;

        Ok(FailJobOutput {
            job_id: input.job_id,
            final_state: JobState::Failed,
            reason: input.reason,
            failed_at: chrono::Utc::now(),
        })
    }
}

pub struct CancelJobActivity {
    repository: Arc<dyn JobRepository + Send + Sync>,
}

impl CancelJobActivity {
    pub fn new(repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl crate::saga::bridge::command_bus::Activity for CancelJobActivity {
    type Input = CancelJobInput;
    type Output = CancelJobOutput;
    type Error = JobStateMachineError;

    fn activity_type_id(&self) -> &'static str {
        "cancel-job"
    }

    fn task_queue(&self) -> Option<&str> {
        Some("job-management")
    }

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job = self
            .repository
            .find_by_id(&input.job_id)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?
            .ok_or(JobStateMachineError::JobNotFound)?;

        if !job.state().can_transition_to(&JobState::Cancelled) {
            return Err(JobStateMachineError::InvalidTransition);
        }

        self.repository
            .update_state(&input.job_id, JobState::Cancelled)
            .await
            .map_err(|_| JobStateMachineError::StateUpdateFailed)?;

        Ok(CancelJobOutput {
            job_id: input.job_id,
            final_state: JobState::Cancelled,
            cancelled_at: chrono::Utc::now(),
        })
    }
}

pub struct JobStateTransitions;

impl JobStateTransitions {
    pub fn is_valid(current: &JobState, desired: &JobState) -> bool {
        current.can_transition_to(desired)
    }

    pub fn is_terminal(state: &JobState) -> bool {
        state.is_terminal()
    }

    pub fn is_in_progress(state: &JobState) -> bool {
        state.is_in_progress()
    }
}

pub mod flows {
    use super::*;

    pub fn success_flow() -> Vec<JobState> {
        vec![
            JobState::Pending,
            JobState::Assigned,
            JobState::Running,
            JobState::Succeeded,
        ]
    }

    pub fn failure_flow() -> Vec<JobState> {
        vec![
            JobState::Pending,
            JobState::Assigned,
            JobState::Running,
            JobState::Failed,
        ]
    }

    pub fn cancellation_flow() -> Vec<JobState> {
        vec![JobState::Pending, JobState::Cancelled]
    }

    pub fn timeout_flow() -> Vec<JobState> {
        vec![JobState::Pending, JobState::Assigned, JobState::Timeout]
    }

    pub fn quick_schedule_flow() -> Vec<JobState> {
        vec![
            JobState::Pending,
            JobState::Scheduled,
            JobState::Running,
            JobState::Succeeded,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::JobSpec;

    fn make_test_job_spec() -> JobSpec {
        JobSpec::new(vec!["echo".to_string(), "hello".to_string()])
    }

    #[test]
    fn test_create_job_input() {
        let spec = make_test_job_spec();
        let input = CreateJobInput::new(spec, Some("test-saga".to_string()));
        assert!(input.saga_id.is_some());
    }

    #[test]
    fn test_terminal_states() {
        assert!(JobState::Succeeded.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
        assert!(JobState::Timeout.is_terminal());
        assert!(!JobState::Pending.is_terminal());
        assert!(!JobState::Running.is_terminal());
    }

    #[test]
    fn test_in_progress_states() {
        assert!(JobState::Pending.is_in_progress());
        assert!(JobState::Assigned.is_in_progress());
        assert!(JobState::Running.is_in_progress());
        assert!(!JobState::Succeeded.is_in_progress());
        assert!(!JobState::Failed.is_in_progress());
    }
}
