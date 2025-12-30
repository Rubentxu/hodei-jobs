//! Execution Saga
//!
//! Saga para la ejecución de jobs en workers.

use crate::saga::{Saga, SagaContext, SagaResult, SagaStep, SagaType};
use crate::shared_kernel::JobId;
use async_trait::async_trait;
use chrono::Utc;
use std::time::Duration;

// ============================================================================
// ExecutionSaga
// ============================================================================

/// Saga para ejecutar un job en un worker.
#[derive(Debug, Clone)]
pub struct ExecutionSaga {
    pub job_id: JobId,
    pub worker_id: Option<JobId>,
}

impl ExecutionSaga {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self {
            job_id,
            worker_id: None,
        }
    }

    #[inline]
    pub fn with_worker(job_id: JobId, worker_id: JobId) -> Self {
        Self {
            job_id,
            worker_id: Some(worker_id),
        }
    }
}

impl Saga for ExecutionSaga {
    fn saga_type(&self) -> SagaType {
        SagaType::Execution
    }

    fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
        vec![
            Box::new(ValidateJobStep::new(self.job_id.clone())),
            Box::new(AssignWorkerStep::new(
                self.job_id.clone(),
                self.worker_id.clone(),
            )),
            Box::new(ExecuteJobStep::new(self.job_id.clone())),
            Box::new(CompleteJobStep::new(self.job_id.clone())),
        ]
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(60))
    }
}

// ============================================================================
// ValidateJobStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct ValidateJobStep {
    job_id: JobId,
}

impl ValidateJobStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ValidateJobStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "ValidateJob"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata("execution_job_id", &self.job_id.to_string())
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        true
    }

    #[inline]
    fn has_compensation(&self) -> bool {
        false
    }
}

// ============================================================================
// AssignWorkerStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct AssignWorkerStep {
    job_id: JobId,
    worker_id: Option<JobId>,
}

impl AssignWorkerStep {
    #[inline]
    pub fn new(job_id: JobId, worker_id: Option<JobId>) -> Self {
        Self { job_id, worker_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for AssignWorkerStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "AssignWorker"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        let worker_id = self.worker_id.clone().unwrap_or_else(JobId::new);
        context
            .set_metadata("execution_worker_id", &worker_id.to_string())
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        false
    }
}

// ============================================================================
// ExecuteJobStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct ExecuteJobStep {
    job_id: JobId,
}

impl ExecuteJobStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for ExecuteJobStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "ExecuteJob"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata("job_executed_at", &Utc::now().to_rfc3339())
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        true
    }
}

// ============================================================================
// CompleteJobStep
// ============================================================================

#[derive(Debug, Clone)]
pub struct CompleteJobStep {
    job_id: JobId,
}

impl CompleteJobStep {
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        Self { job_id }
    }
}

#[async_trait::async_trait]
impl SagaStep for CompleteJobStep {
    type Output = ();

    #[inline]
    fn name(&self) -> &'static str {
        "CompleteJob"
    }

    async fn execute(&self, context: &mut SagaContext) -> SagaResult<Self::Output> {
        context
            .set_metadata("job_completed_at", &Utc::now().to_rfc3339())
            .map_err(|e| crate::saga::SagaError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn compensate(&self, _output: &Self::Output) -> SagaResult<()> {
        Ok(())
    }

    #[inline]
    fn is_idempotent(&self) -> bool {
        true
    }

    #[inline]
    fn has_compensation(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_saga_should_have_four_steps() {
        let saga = ExecutionSaga::new(JobId::new());
        let steps = saga.steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name(), "ValidateJob");
        assert_eq!(steps[1].name(), "AssignWorker");
        assert_eq!(steps[2].name(), "ExecuteJob");
        assert_eq!(steps[3].name(), "CompleteJob");
    }

    #[test]
    fn execution_saga_has_correct_type() {
        let saga = ExecutionSaga::new(JobId::new());
        assert_eq!(saga.saga_type(), SagaType::Execution);
    }

    #[test]
    fn execution_saga_has_timeout() {
        let saga = ExecutionSaga::new(JobId::new());
        assert!(saga.timeout().is_some());
        assert_eq!(saga.timeout().unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn validate_job_step_has_no_compensation() {
        let step = ValidateJobStep::new(JobId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }

    #[test]
    fn execute_job_step_is_idempotent() {
        let step = ExecuteJobStep::new(JobId::new());
        assert!(step.is_idempotent());
    }

    #[test]
    fn complete_job_step_has_no_compensation() {
        let step = CompleteJobStep::new(JobId::new());
        assert!(!step.has_compensation());
        assert!(step.is_idempotent());
    }
}
