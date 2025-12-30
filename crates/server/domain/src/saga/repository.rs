//! Saga Repository Trait
//!
//! Abstraction for saga persistence operations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::saga::{SagaContext, SagaId, SagaState, SagaType};

/// Repository for saga instance persistence
///
/// Provides operations for storing and retrieving saga instances
/// used in the Saga pattern for recovery and auditing.
#[async_trait::async_trait]
pub trait SagaRepository: Send + Sync {
    /// Error type for repository operations
    type Error: std::fmt::Debug + Send + Sync;

    /// Save a new saga instance
    async fn save(&self, context: &SagaContext) -> Result<(), Self::Error>;

    /// Find a saga by its ID
    async fn find_by_id(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error>;

    /// Find sagas by type
    async fn find_by_type(&self, saga_type: SagaType) -> Result<Vec<SagaContext>, Self::Error>;

    /// Find sagas by state
    async fn find_by_state(&self, state: SagaState) -> Result<Vec<SagaContext>, Self::Error>;

    /// Find sagas by correlation ID
    async fn find_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error>;

    /// Update saga state
    async fn update_state(
        &self,
        saga_id: &SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error>;

    /// Mark saga as compensating
    async fn mark_compensating(&self, saga_id: &SagaId) -> Result<(), Self::Error>;

    /// Delete a completed saga (for cleanup)
    async fn delete(&self, saga_id: &SagaId) -> Result<bool, Self::Error>;

    // ============ Step Operations ============

    /// Save a saga step
    async fn save_step(&self, step: &SagaStepData) -> Result<(), Self::Error>;

    /// Find a step by ID
    async fn find_step_by_id(
        &self,
        step_id: &SagaStepId,
    ) -> Result<Option<SagaStepData>, Self::Error>;

    /// Find steps for a saga
    async fn find_steps_by_saga_id(
        &self,
        saga_id: &SagaId,
    ) -> Result<Vec<SagaStepData>, Self::Error>;

    /// Update step state
    async fn update_step_state(
        &self,
        step_id: &SagaStepId,
        state: SagaStepState,
        output: Option<serde_json::Value>,
    ) -> Result<(), Self::Error>;

    /// Update step compensation data
    async fn update_step_compensation(
        &self,
        step_id: &SagaStepId,
        compensation_data: serde_json::Value,
    ) -> Result<(), Self::Error>;

    // ============ Statistics ============

    /// Count active sagas
    async fn count_active(&self) -> Result<u64, Self::Error>;

    /// Count sagas by type and state
    async fn count_by_type_and_state(
        &self,
        saga_type: SagaType,
        state: SagaState,
    ) -> Result<u64, Self::Error>;

    /// Get average saga duration
    async fn avg_duration(&self) -> Result<Option<std::time::Duration>, Self::Error>;

    // ============ Cleanup ============

    /// Cleanup old completed sagas
    async fn cleanup_completed(&self, older_than: std::time::Duration) -> Result<u64, Self::Error>;
}

/// Unique identifier for a saga step
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SagaStepId(pub Uuid);

impl SagaStepId {
    /// Creates a new SagaStepId
    #[inline]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID
    #[inline]
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for SagaStepId {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SagaStepId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// State of a saga step
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaStepState {
    /// Step has not been executed yet
    Pending,
    /// Step is currently executing
    InProgress,
    /// Step completed successfully
    Completed,
    /// Step failed
    Failed,
    /// Step is compensating (rolling back)
    Compensating,
    /// Step was compensated
    Compensated,
}

impl fmt::Display for SagaStepState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SagaStepState::Pending => write!(f, "PENDING"),
            SagaStepState::InProgress => write!(f, "IN_PROGRESS"),
            SagaStepState::Completed => write!(f, "COMPLETED"),
            SagaStepState::Failed => write!(f, "FAILED"),
            SagaStepState::Compensating => write!(f, "COMPENSATING"),
            SagaStepState::Compensated => write!(f, "COMPENSATED"),
        }
    }
}

/// Data for a saga step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStepData {
    /// Unique step ID
    pub step_id: SagaStepId,
    /// Parent saga ID
    pub saga_id: SagaId,
    /// Step name (e.g., "CreateWorker")
    pub step_name: String,
    /// Order in the saga sequence
    pub step_order: i32,
    /// Current step state
    pub state: SagaStepState,
    /// Input data for the step (JSON)
    pub input_data: Option<serde_json::Value>,
    /// Output data from the step (JSON)
    pub output_data: Option<serde_json::Value>,
    /// Compensation data (stored after successful execution)
    pub compensation_data: Option<serde_json::Value>,
    /// When the step started
    pub started_at: Option<DateTime<Utc>>,
    /// When the step completed
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Number of retry attempts
    pub retry_count: i32,
    /// When the record was created
    pub created_at: DateTime<Utc>,
    /// When the record was last updated
    pub updated_at: DateTime<Utc>,
}

impl SagaStepData {
    /// Creates a new step data record
    #[inline]
    pub fn new(
        saga_id: SagaId,
        step_name: String,
        step_order: i32,
        input_data: Option<serde_json::Value>,
    ) -> Self {
        let now = Utc::now();
        Self {
            step_id: SagaStepId::new(),
            saga_id,
            step_name,
            step_order,
            state: SagaStepState::Pending,
            input_data,
            output_data: None,
            compensation_data: None,
            started_at: None,
            completed_at: None,
            error_message: None,
            retry_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Creates a new step data record with custom step ID
    #[inline]
    pub fn new_with_order(
        saga_id: SagaId,
        step_name: String,
        step_order: i32,
        input_data: Option<serde_json::Value>,
        step_id: SagaStepId,
    ) -> Self {
        let now = Utc::now();
        Self {
            step_id,
            saga_id,
            step_name,
            step_order,
            state: SagaStepState::Pending,
            input_data,
            output_data: None,
            compensation_data: None,
            started_at: None,
            completed_at: None,
            error_message: None,
            retry_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Mark step as in progress
    #[inline]
    pub fn mark_in_progress(&mut self) {
        self.state = SagaStepState::InProgress;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark step as completed
    #[inline]
    pub fn mark_completed(&mut self, output: serde_json::Value) {
        self.state = SagaStepState::Completed;
        self.output_data = Some(output);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark step as failed
    #[inline]
    pub fn mark_failed(&mut self, error_message: String) {
        self.state = SagaStepState::Failed;
        self.error_message = Some(error_message);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::SagaType;
    use std::sync::{Arc, Mutex};

    // Mock implementation for testing
    struct MockSagaRepository {
        sagas: Mutex<Vec<SagaContext>>,
        steps: Mutex<Vec<SagaStepData>>,
    }

    impl MockSagaRepository {
        fn new() -> Self {
            Self {
                sagas: Mutex::new(Vec::new()),
                steps: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl SagaRepository for MockSagaRepository {
        type Error = std::io::Error;

        async fn save(&self, context: &SagaContext) -> Result<(), Self::Error> {
            let mut sagas = self.sagas.lock().unwrap();
            sagas.push(context.clone());
            Ok(())
        }

        async fn find_by_id(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
            let sagas = self.sagas.lock().unwrap();
            Ok(sagas.iter().find(|s| s.saga_id == *saga_id).cloned())
        }

        async fn find_by_type(&self, saga_type: SagaType) -> Result<Vec<SagaContext>, Self::Error> {
            let sagas = self.sagas.lock().unwrap();
            Ok(sagas
                .iter()
                .filter(|s| s.saga_type == saga_type)
                .cloned()
                .collect())
        }

        async fn find_by_state(&self, state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
            let sagas = self.sagas.lock().unwrap();
            Ok(sagas
                .iter()
                .filter(|s| match state {
                    SagaState::Pending => s.current_step == 0,
                    SagaState::InProgress => s.current_step > 0 && !s.is_compensating,
                    _ => false,
                })
                .cloned()
                .collect())
        }

        async fn find_by_correlation_id(
            &self,
            correlation_id: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            let sagas = self.sagas.lock().unwrap();
            Ok(sagas
                .iter()
                .filter(|s| s.correlation_id.as_deref() == Some(correlation_id))
                .cloned()
                .collect())
        }

        async fn update_state(
            &self,
            saga_id: &SagaId,
            state: SagaState,
            _error_message: Option<String>,
        ) -> Result<(), Self::Error> {
            let mut sagas = self.sagas.lock().unwrap();
            if let Some(saga) = sagas.iter_mut().find(|s| s.saga_id == *saga_id) {
                saga.current_step = if state == SagaState::Completed {
                    usize::MAX
                } else {
                    saga.current_step
                };
            }
            Ok(())
        }

        async fn mark_compensating(&self, _saga_id: &SagaId) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn delete(&self, _saga_id: &SagaId) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn save_step(&self, step: &SagaStepData) -> Result<(), Self::Error> {
            let mut steps = self.steps.lock().unwrap();
            steps.push(step.clone());
            Ok(())
        }

        async fn find_step_by_id(
            &self,
            step_id: &SagaStepId,
        ) -> Result<Option<SagaStepData>, Self::Error> {
            let steps = self.steps.lock().unwrap();
            Ok(steps.iter().find(|s| s.step_id == *step_id).cloned())
        }

        async fn find_steps_by_saga_id(
            &self,
            saga_id: &SagaId,
        ) -> Result<Vec<SagaStepData>, Self::Error> {
            let steps = self.steps.lock().unwrap();
            Ok(steps
                .iter()
                .filter(|s| s.saga_id == *saga_id)
                .cloned()
                .collect())
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
            let sagas = self.sagas.lock().unwrap();
            Ok(sagas
                .iter()
                .filter(|s| s.current_step > 0 || s.error_message.is_none())
                .count() as u64)
        }

        async fn count_by_type_and_state(
            &self,
            _saga_type: SagaType,
            _state: SagaState,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn avg_duration(&self) -> Result<Option<std::time::Duration>, Self::Error> {
            Ok(None)
        }

        async fn cleanup_completed(
            &self,
            _older_than: std::time::Duration,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_save_saga() {
        let repo = Arc::new(MockSagaRepository::new());
        let context = SagaContext::new(
            SagaId::new(),
            SagaType::Provisioning,
            Some("corr-1".to_string()),
            Some("actor-1".to_string()),
        );

        repo.save(&context).await.unwrap();

        let found = repo.find_by_id(&context.saga_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().saga_type, SagaType::Provisioning);
    }

    #[tokio::test]
    async fn test_find_by_type() {
        let repo = Arc::new(MockSagaRepository::new());
        let saga1 = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        let saga2 = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        repo.save(&saga1).await.unwrap();
        repo.save(&saga2).await.unwrap();

        let provisioning_sagas = repo.find_by_type(SagaType::Provisioning).await.unwrap();
        assert_eq!(provisioning_sagas.len(), 1);
        assert_eq!(provisioning_sagas[0].saga_type, SagaType::Provisioning);
    }

    #[tokio::test]
    async fn test_save_steps() {
        let repo = Arc::new(MockSagaRepository::new());
        let saga_id = SagaId::new();

        let step = SagaStepData::new(
            saga_id.clone(),
            "CreateInfrastructure".to_string(),
            1,
            Some(serde_json::json!({"resource_id": "pod-123"})),
        );

        repo.save_step(&step).await.unwrap();

        let found = repo.find_step_by_id(&step.step_id).await.unwrap().unwrap();
        assert_eq!(found.step_name, "CreateInfrastructure");
    }

    #[tokio::test]
    async fn test_saga_step_data_creation() {
        let saga_id = SagaId::new();
        let mut step = SagaStepData::new(
            saga_id.clone(),
            "TestStep".to_string(),
            1,
            Some(serde_json::json!({"key": "value"})),
        );

        assert_eq!(step.step_name, "TestStep");
        assert_eq!(step.step_order, 1);
        assert_eq!(step.state, SagaStepState::Pending);

        step.mark_in_progress();
        assert_eq!(step.state, SagaStepState::InProgress);
        assert!(step.started_at.is_some());

        step.mark_completed(serde_json::json!({"result": "success"}));
        assert_eq!(step.state, SagaStepState::Completed);
        assert!(step.output_data.is_some());
        assert!(step.completed_at.is_some());

        let mut failed_step = SagaStepData::new(saga_id, "FailingStep".to_string(), 2, None);
        failed_step.mark_failed("Test error".to_string());
        assert_eq!(failed_step.state, SagaStepState::Failed);
        assert_eq!(failed_step.error_message, Some("Test error".to_string()));
    }
}
