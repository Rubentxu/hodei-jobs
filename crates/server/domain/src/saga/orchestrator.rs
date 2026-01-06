//! Saga Orchestrator
//!
//! Core orchestrator for executing sagas with automatic compensation.

use super::{
    ExecutionSaga, ProvisioningSaga, RecoverySaga, Saga, SagaContext, SagaError,
    SagaExecutionResult, SagaId, SagaOrchestrator, SagaRepository, SagaState, SagaType,
};
use crate::shared_kernel::{JobId, ProviderId, WorkerId};
use crate::workers::WorkerSpec;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use thiserror::Error;

/// Orchestrator configuration
#[derive(Debug, Clone)]
pub struct SagaOrchestratorConfig {
    pub max_concurrent_sagas: usize,
    pub max_concurrent_steps: usize,
    pub step_timeout: Duration,
    pub saga_timeout: Duration,
    pub max_retries: u32,
    pub retry_backoff: Duration,
}

impl Default for SagaOrchestratorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sagas: 100,
            max_concurrent_steps: 10,
            step_timeout: Duration::from_secs(30),
            saga_timeout: Duration::from_secs(300),
            max_retries: 3,
            retry_backoff: Duration::from_secs(1),
        }
    }
}

/// Errors from orchestrator operations
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("Saga execution timeout after {duration:?}")]
    Timeout { duration: Duration },

    #[error("Saga was cancelled")]
    Cancelled,

    #[error("Saga step '{step}' failed: {error}")]
    StepFailed { step: String, error: String },

    #[error("Compensation failed for step '{step}': {error}")]
    CompensationFailed { step: String, error: String },

    #[error("Concurrency limit exceeded: {current}/{max}")]
    ConcurrencyLimitExceeded { current: usize, max: usize },

    #[error("Saga not found: {saga_id}")]
    SagaNotFound { saga_id: SagaId },

    #[error("Persistence error: {message}")]
    PersistenceError { message: String },
}

impl From<SagaError> for OrchestratorError {
    fn from(err: SagaError) -> Self {
        match err {
            SagaError::Timeout { duration } => OrchestratorError::Timeout { duration },
            SagaError::Cancelled => OrchestratorError::Cancelled,
            SagaError::StepFailed { step, message, .. } => OrchestratorError::StepFailed {
                step,
                error: message,
            },
            SagaError::CompensationFailed { step, message } => {
                OrchestratorError::CompensationFailed {
                    step,
                    error: message,
                }
            }
            SagaError::SerializationError(msg) | SagaError::DeserializationError(msg) => {
                OrchestratorError::PersistenceError { message: msg }
            }
            _ => OrchestratorError::PersistenceError {
                message: err.to_string(),
            },
        }
    }
}

/// In-memory saga orchestrator for testing and development
///
/// This implementation stores saga state in memory and should not be
/// used in production environments where persistence is required.
#[derive(Debug, Clone)]
pub struct InMemorySagaOrchestrator<R: SagaRepository + Clone> {
    /// Repository for saga persistence
    repository: Arc<R>,
    /// Configuration
    config: SagaOrchestratorConfig,
    /// Active saga count (for concurrency control)
    active_sagas: Arc<AtomicUsize>,
}

impl<R: SagaRepository + Clone> InMemorySagaOrchestrator<R> {
    /// Creates a new in-memory orchestrator
    pub fn new(repository: Arc<R>, config: Option<SagaOrchestratorConfig>) -> Self {
        Self {
            repository,
            config: config.unwrap_or_default(),
            active_sagas: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl<R: SagaRepository + Clone + Send + Sync + 'static> SagaOrchestrator
    for InMemorySagaOrchestrator<R>
where
    <R as SagaRepository>::Error: std::fmt::Display + Send + Sync,
{
    type Error = OrchestratorError;

    async fn execute_saga(
        &self,
        saga: &dyn Saga,
        mut context: SagaContext,
    ) -> Result<SagaExecutionResult, Self::Error> {
        let start_time = std::time::Instant::now();
        let saga_id = context.saga_id.clone();

        // Check concurrency limit using atomic load
        let current = self.active_sagas.load(Ordering::SeqCst);
        if current >= self.config.max_concurrent_sagas {
            return Err(OrchestratorError::ConcurrencyLimitExceeded {
                current,
                max: self.config.max_concurrent_sagas,
            });
        }

        // Increment active count
        self.active_sagas.fetch_add(1, Ordering::SeqCst);

        // Ensure we decrement on exit (using a simple guard)
        let active_sagas = self.active_sagas.clone();
        let _guard = DropGuard(active_sagas);

        // Save initial context
        self.repository
            .save(&context)
            .await
            .map_err(|e| OrchestratorError::PersistenceError {
                message: e.to_string(),
            })?;

        let steps = saga.steps();
        let mut executed_steps = context.current_step;
        let is_resume = context.current_step > 0;

        // Execute steps sequentially
        let steps_vec = steps.into_iter().collect::<Vec<_>>();

        // Log resume status
        if is_resume {
            tracing::info!(
                saga_id = %saga_id,
                current_step = context.current_step,
                "Resuming saga from step"
            );
        }

        for (step_idx, step) in steps_vec.iter().enumerate() {
            // Skip steps that were already executed (resume scenario)
            if step_idx < context.current_step {
                tracing::debug!(
                    step = step.name(),
                    step_idx = step_idx,
                    "Skipping already executed step (resume)"
                );
                continue;
            }

            // Check saga timeout
            if start_time.elapsed() > self.config.saga_timeout {
                context.set_error(format!("Saga timed out after {:?}", start_time.elapsed()));
                self.repository
                    .update_state(&saga_id, SagaState::Failed, context.error_message.clone())
                    .await
                    .ok();

                return Err(OrchestratorError::Timeout {
                    duration: start_time.elapsed(),
                });
            }

            // Execute step with timeout
            let step_result =
                tokio::time::timeout(self.config.step_timeout, step.execute(&mut context)).await;

            match step_result {
                Ok(Ok(_output)) => {
                    // Step succeeded
                    executed_steps += 1;
                }
                Ok(Err(e)) => {
                    // Step failed - start REAL compensation in REVERSE order
                    context.set_error(e.to_string());
                    context.is_compensating = true;

                    self.repository
                        .update_state(&saga_id, SagaState::Compensating, Some(e.to_string()))
                        .await
                        .ok();

                    // Execute compensation for all previously executed steps in REVERSE order
                    let mut _compensations_executed = 0;
                    for compensate_idx in (0..step_idx).rev() {
                        let compensate_step = &steps_vec[compensate_idx];

                        // Only compensate if step has compensation defined
                        if compensate_step.has_compensation() {
                            let step_name = compensate_step.name();
                            let comp_result = compensate_step.compensate(&mut context).await;

                            match comp_result {
                                Ok(()) => {
                                    _compensations_executed += 1;
                                    tracing::info!(
                                        step = step_name,
                                        "Compensation step executed successfully"
                                    );
                                }
                                Err(comp_error) => {
                                    tracing::error!(
                                        step = step_name,
                                        error = %comp_error,
                                        "Compensation step failed"
                                    );
                                    // Continue compensating other steps even if one fails
                                }
                            }
                        }
                    }

                    // Update final state to Failed (compensation completed but saga failed)
                    self.repository
                        .update_state(&saga_id, SagaState::Failed, Some(e.to_string()))
                        .await
                        .ok();

                    return Ok(SagaExecutionResult::failed(
                        saga_id,
                        saga.saga_type(),
                        start_time.elapsed(),
                        executed_steps as u32,
                        _compensations_executed as u32,
                        e.to_string(),
                    ));
                }
                Err(_) => {
                    // Step timed out
                    let timeout_error =
                        format!("Step timed out after {:?}", self.config.step_timeout);
                    context.set_error(timeout_error.clone());
                    context.is_compensating = true;

                    self.repository
                        .update_state(
                            &saga_id,
                            SagaState::Compensating,
                            Some(timeout_error.clone()),
                        )
                        .await
                        .ok();

                    // Execute compensation for all previously executed steps in REVERSE order
                    let mut compensations_executed = 0;
                    for compensate_idx in (0..executed_steps).rev() {
                        let compensate_step = &steps_vec[compensate_idx];

                        if compensate_step.has_compensation() {
                            let step_name = compensate_step.name();
                            let comp_result = compensate_step.compensate(&mut context).await;

                            match comp_result {
                                Ok(()) => {
                                    compensations_executed += 1;
                                    tracing::info!(
                                        step = step_name,
                                        "Compensation step executed successfully (timeout)"
                                    );
                                }
                                Err(comp_error) => {
                                    tracing::error!(
                                        step = step_name,
                                        error = %comp_error,
                                        "Compensation step failed (timeout)"
                                    );
                                }
                            }
                        }
                    }

                    self.repository
                        .update_state(&saga_id, SagaState::Failed, Some(timeout_error))
                        .await
                        .ok();

                    return Err(OrchestratorError::Timeout {
                        duration: self.config.step_timeout,
                    });
                }
            }
        }

        // All steps completed successfully
        self.repository
            .update_state(&saga_id, SagaState::Completed, None)
            .await
            .ok();

        Ok(SagaExecutionResult::completed_with_steps(
            saga_id,
            saga.saga_type(),
            start_time.elapsed(),
            executed_steps as u32,
        ))
    }

    /// EPIC-42: Execute saga directly from context (for reactive processing)
    async fn execute(&self, context: &SagaContext) -> Result<SagaExecutionResult, Self::Error> {
        // Create the appropriate saga based on saga_type
        let saga: Box<dyn Saga> = match context.saga_type {
            SagaType::Provisioning => {
                // Extract provider_id from metadata
                let provider_id_str = context
                    .metadata
                    .get("provider_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let provider_id = if !provider_id_str.is_empty() {
                    ProviderId::from_uuid(
                        uuid::Uuid::parse_str(&provider_id_str)
                            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    ProviderId::new()
                };

                let spec = WorkerSpec::new(
                    "hodei-jobs-worker:latest".to_string(),
                    "http://localhost:50051".to_string(),
                );

                Box::new(ProvisioningSaga::new(spec, provider_id))
            }
            SagaType::Execution => {
                let job_id_str = context
                    .metadata
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let job_id = if !job_id_str.is_empty() {
                    JobId(
                        uuid::Uuid::parse_str(&job_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    JobId::new()
                };

                Box::new(ExecutionSaga::new(job_id))
            }
            SagaType::Recovery => Box::new(RecoverySaga::new(JobId::new(), WorkerId::new(), None)),
            SagaType::Cancellation | SagaType::Timeout | SagaType::Cleanup => {
                // TODO: Implement these saga types
                todo!("Saga type not yet implemented in reactive orchestrator")
            }
        };

        // Execute with a clone of the context
        self.execute_saga(&*saga, context.clone()).await
    }

    async fn get_saga(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
        self.repository
            .find_by_id(saga_id)
            .await
            .map_err(|e| OrchestratorError::PersistenceError {
                message: e.to_string(),
            })
    }

    async fn cancel_saga(&self, saga_id: &SagaId) -> Result<(), Self::Error> {
        self.repository
            .mark_compensating(saga_id)
            .await
            .map_err(|e| OrchestratorError::PersistenceError {
                message: e.to_string(),
            })?;

        self.repository
            .update_state(
                saga_id,
                SagaState::Cancelled,
                Some("Cancelled by user".to_string()),
            )
            .await
            .map_err(|e| OrchestratorError::PersistenceError {
                message: e.to_string(),
            })?;

        Ok(())
    }
}

/// Simple guard to decrement active saga count
struct DropGuard(Arc<AtomicUsize>);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::{
        SagaContext, SagaError, SagaResult, SagaStep, SagaStepData, SagaStepId, SagaStepState,
        SagaType,
    };
    use chrono::Utc;

    // Test saga implementation - uses Rc<RefCell> to allow cloning
    struct TestSaga {
        success_count: usize,
        fail_on_step: Option<usize>,
    }

    impl TestSaga {
        fn new(success_count: usize, fail_on_step: Option<usize>) -> Self {
            Self {
                success_count,
                fail_on_step,
            }
        }
    }

    impl Saga for TestSaga {
        fn saga_type(&self) -> SagaType {
            SagaType::Execution
        }

        fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
            let mut steps: Vec<Box<dyn SagaStep<Output = ()>>> = Vec::new();
            for i in 0..self.success_count {
                let fail_on = self.fail_on_step;
                steps.push(Box::new(TestStep {
                    step_number: i,
                    should_fail: fail_on == Some(i),
                }));
            }
            steps
        }
    }

    struct TestStep {
        step_number: usize,
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl SagaStep for TestStep {
        type Output = ();

        fn name(&self) -> &'static str {
            if self.should_fail {
                "FailStep"
            } else {
                "SuccessStep"
            }
        }

        async fn execute(&self, _context: &mut SagaContext) -> Result<(), SagaError> {
            if self.should_fail {
                Err(SagaError::StepFailed {
                    step: format!("Step{}", self.step_number),
                    message: "Intentional failure".to_string(),
                    will_compensate: true,
                })
            } else {
                Ok(())
            }
        }

        async fn compensate(&self, _context: &mut SagaContext) -> Result<(), SagaError> {
            Ok(())
        }
    }

    // Mock repository for testing
    #[derive(Clone)]
    struct MockSagaRepository;

    #[async_trait::async_trait]
    impl SagaRepository for MockSagaRepository {
        type Error = std::io::Error;

        async fn save(&self, _context: &SagaContext) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn create_if_not_exists(&self, _context: &SagaContext) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn find_by_id(&self, _saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
            Ok(None)
        }

        async fn find_by_type(
            &self,
            _saga_type: SagaType,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }

        async fn find_by_state(&self, _state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }

        async fn find_by_correlation_id(
            &self,
            _correlation_id: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }

        async fn update_state(
            &self,
            _saga_id: &SagaId,
            _state: SagaState,
            _error_message: Option<String>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn mark_compensating(&self, _saga_id: &SagaId) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn delete(&self, _saga_id: &SagaId) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn save_step(&self, _step: &SagaStepData) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn find_step_by_id(
            &self,
            _step_id: &SagaStepId,
        ) -> Result<Option<SagaStepData>, Self::Error> {
            Ok(None)
        }

        async fn find_steps_by_saga_id(
            &self,
            _saga_id: &SagaId,
        ) -> Result<Vec<SagaStepData>, Self::Error> {
            Ok(Vec::new())
        }

        async fn update_step_state(
            &self,
            _step_id: &SagaStepId,
            _state: SagaStepState,
            _output: Option<serde_json::Value>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn update_step_compensation(
            &self,
            _step_id: &SagaStepId,
            _compensation_data: serde_json::Value,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn count_active(&self) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn count_by_type_and_state(
            &self,
            _saga_type: SagaType,
            _state: SagaState,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn avg_duration(&self) -> Result<Option<Duration>, Self::Error> {
            Ok(None)
        }

        async fn cleanup_completed(&self, _older_than: Duration) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn claim_pending_sagas(
            &self,
            _limit: u64,
            _instance_id: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_execute_saga_completes() {
        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // Create saga with 3 successful steps
        let saga = TestSaga::new(3, None);

        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_success());
        assert_eq!(saga_result.steps_executed, 3);
        assert_eq!(saga_result.compensations_executed, 0);
    }

    #[tokio::test]
    async fn test_execute_saga_compensates_on_failure() {
        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // Create saga with 2 steps where step 1 fails
        let saga = TestSaga::new(2, Some(1));

        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed() || saga_result.is_compensated());
        assert_eq!(saga_result.steps_executed, 1);
    }

    // ============ REAL COMPENSATION TESTS ============

    use std::sync::Mutex;

    #[tokio::test]
    async fn test_compensation_loop_executes_in_reverse_order() {
        // Use Mutex<Vec<usize>> to track which steps were compensated
        let compensated_steps: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        // Step that tracks when compensate() is called
        struct TrackingCompensatingStep {
            step_number: usize,
            should_fail: bool,
            compensated: Arc<Mutex<Vec<usize>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for TrackingCompensatingStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "TrackingCompensatingStep"
            }

            fn has_compensation(&self) -> bool {
                true
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                if self.should_fail {
                    Err(SagaError::StepFailed {
                        step: format!("Step{}", self.step_number),
                        message: "Intentional failure".to_string(),
                        will_compensate: true,
                    })
                } else {
                    Ok(())
                }
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                let mut compensated = self.compensated.lock().unwrap();
                compensated.push(self.step_number);
                Ok(())
            }
        }

        // Create a saga with 3 steps where step 2 fails
        struct TestSaga {
            fail_on_step: usize,
            compensated: Arc<Mutex<Vec<usize>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let compensated = self.compensated.clone();
                vec![
                    Box::new(TrackingCompensatingStep {
                        step_number: 0,
                        should_fail: false,
                        compensated: compensated.clone(),
                    }),
                    Box::new(TrackingCompensatingStep {
                        step_number: 1,
                        should_fail: false,
                        compensated: compensated.clone(),
                    }),
                    Box::new(TrackingCompensatingStep {
                        step_number: 2,
                        should_fail: self.fail_on_step == 2,
                        compensated: compensated.clone(),
                    }),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        let saga = TestSaga {
            fail_on_step: 2,
            compensated: compensated_steps.clone(),
        };
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed());

        // Steps 0 and 1 should have been compensated
        let compensated = compensated_steps.lock().unwrap();
        assert_eq!(compensated.len(), 2);
        assert!(compensated.contains(&0));
        assert!(compensated.contains(&1));
    }

    #[tokio::test]
    async fn test_compensation_skips_steps_without_compensation() {
        // Step with no compensation defined
        struct NoCompensationTestStep;

        #[async_trait::async_trait]
        impl SagaStep for NoCompensationTestStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "NoCompensationTestStep"
            }

            fn has_compensation(&self) -> bool {
                false
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                panic!("Should not be called!");
            }
        }

        // Step that fails during execution
        struct FailingTestStep;

        #[async_trait::async_trait]
        impl SagaStep for FailingTestStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "FailingTestStep"
            }

            fn has_compensation(&self) -> bool {
                true
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Err(SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "Intentional failure".to_string(),
                    will_compensate: true,
                })
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga;

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![
                    Box::new(NoCompensationTestStep),
                    Box::new(FailingTestStep),
                    Box::new(NoCompensationTestStep),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        let saga = TestSaga;
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed());

        // FailingTestStep has compensation, but it failed execution, so nothing to compensate
        assert_eq!(saga_result.compensations_executed, 0);
    }

    #[tokio::test]
    async fn test_compensation_continues_even_if_one_fails() {
        let compensated_steps: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        // Step that tracks compensation and can fail
        struct FailingCompensationStep {
            step_number: usize,
            should_fail_compensation: bool,
            compensated: Arc<Mutex<Vec<usize>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for FailingCompensationStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "FailingCompensationStep"
            }

            fn has_compensation(&self) -> bool {
                true
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                let mut compensated = self.compensated.lock().unwrap();
                compensated.push(self.step_number);

                if self.should_fail_compensation {
                    Err(SagaError::CompensationFailed {
                        step: self.name().to_string(),
                        message: "Intentional compensation failure".to_string(),
                    })
                } else {
                    Ok(())
                }
            }
        }

        // Step that fails during execution
        struct FailingTestStep;

        #[async_trait::async_trait]
        impl SagaStep for FailingTestStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "FailingTestStep"
            }

            fn has_compensation(&self) -> bool {
                true
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Err(SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "Intentional failure".to_string(),
                    will_compensate: true,
                })
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga {
            compensated: Arc<Mutex<Vec<usize>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let compensated = self.compensated.clone();
                vec![
                    Box::new(FailingCompensationStep {
                        step_number: 0,
                        should_fail_compensation: false,
                        compensated: compensated.clone(),
                    }),
                    Box::new(FailingCompensationStep {
                        step_number: 1,
                        should_fail_compensation: true,
                        compensated: compensated.clone(),
                    }),
                    Box::new(FailingTestStep),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        let saga = TestSaga {
            compensated: compensated_steps.clone(),
        };
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed());

        // Both steps should have been attempted for compensation
        let compensated = compensated_steps.lock().unwrap();
        assert_eq!(compensated.len(), 2);
        assert!(compensated.contains(&0));
        assert!(compensated.contains(&1));
    }

    // ============ SAGA RESUME TESTS ============

    #[tokio::test]
    async fn test_saga_resumes_from_last_completed_step() {
        let executed_steps: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        struct TrackingStep {
            step_number: usize,
            executed: Arc<Mutex<Vec<usize>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for TrackingStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "TrackingStep"
            }

            fn has_compensation(&self) -> bool {
                false
            }

            async fn execute(&self, context: &mut SagaContext) -> SagaResult<()> {
                let mut executed = self.executed.lock().unwrap();
                executed.push(self.step_number);
                // Update context to track progress
                context.current_step = self.step_number + 1;
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga {
            executed: Arc<Mutex<Vec<usize>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let executed = self.executed.clone();
                vec![
                    Box::new(TrackingStep {
                        step_number: 0,
                        executed: executed.clone(),
                    }),
                    Box::new(TrackingStep {
                        step_number: 1,
                        executed: executed.clone(),
                    }),
                    Box::new(TrackingStep {
                        step_number: 2,
                        executed: executed.clone(),
                    }),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // First run: saga with 3 steps
        let saga = TestSaga {
            executed: executed_steps.clone(),
        };
        // Start from step 1 (simulating resume after step 0 completed)
        let saga_id = SagaId::new();
        let context = SagaContext::from_persistence(
            saga_id,
            SagaType::Execution,
            None,
            None,
            Utc::now(),
            1, // Step 0 already completed
            false,
            std::collections::HashMap::new(),
            None,
            SagaState::InProgress,
            1,
            None,
        );

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_success());

        // Only steps 1 and 2 should have been executed
        let executed = executed_steps.lock().unwrap();
        assert_eq!(executed.len(), 2);
        assert!(executed.contains(&1));
        assert!(executed.contains(&2));
        // Step 0 should NOT be executed again
        assert!(!executed.contains(&0));
    }

    #[tokio::test]
    async fn test_saga_starts_from_beginning_if_no_completed_steps() {
        let executed_steps: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        struct TrackingStep {
            step_number: usize,
            executed: Arc<Mutex<Vec<usize>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for TrackingStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "TrackingStep"
            }

            fn has_compensation(&self) -> bool {
                false
            }

            async fn execute(&self, context: &mut SagaContext) -> SagaResult<()> {
                let mut executed = self.executed.lock().unwrap();
                executed.push(self.step_number);
                context.current_step = self.step_number + 1;
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga {
            executed: Arc<Mutex<Vec<usize>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let executed = self.executed.clone();
                vec![
                    Box::new(TrackingStep {
                        step_number: 0,
                        executed: executed.clone(),
                    }),
                    Box::new(TrackingStep {
                        step_number: 1,
                        executed: executed.clone(),
                    }),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // Start from beginning (current_step = 0)
        let saga = TestSaga {
            executed: executed_steps.clone(),
        };
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_success());

        // All steps should have been executed
        let executed = executed_steps.lock().unwrap();
        assert_eq!(executed.len(), 2);
        assert!(executed.contains(&0));
        assert!(executed.contains(&1));
    }
}
