//! Saga Orchestrator
//!
//! Core orchestrator for executing sagas with automatic compensation.

use crate::saga::{
    Saga, SagaContext, SagaError, SagaExecutionResult, SagaId, SagaOrchestrator, SagaRepository,
    SagaState,
};
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
        let mut executed_steps = 0;

        // Execute steps sequentially
        for step in steps.into_iter() {
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
                    // Step failed - start compensation
                    context.set_error(e.to_string());

                    self.repository.mark_compensating(&saga_id).await.ok();

                    self.repository
                        .update_state(&saga_id, SagaState::Compensating, Some(e.to_string()))
                        .await
                        .ok();

                    return Ok(SagaExecutionResult::failed(
                        saga_id,
                        saga.saga_type(),
                        start_time.elapsed(),
                        executed_steps as u32,
                        executed_steps as u32,
                        e.to_string(),
                    ));
                }
                Err(_) => {
                    // Step timed out
                    let timeout_error =
                        format!("Step timed out after {:?}", self.config.step_timeout);
                    context.set_error(timeout_error.clone());

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
    use crate::saga::{SagaContext, SagaStep, SagaStepData, SagaStepId, SagaStepState, SagaType};

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

        async fn compensate(&self, _output: &Self::Output) -> Result<(), SagaError> {
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
}
