//! # Compensation Tracker
//!
//! Automatic compensation tracking for saga workflows with rollback support.
//! Tracks completed steps and provides automatic compensation on failure.
//!
//! ## Patrón de Compensación
//!
//! ```text
//! Workflow Steps:     [Step 1] → [Step 2] → [Step 3] → [FAIL!]
//!                     ↓           ↓           ↓
//! Compensation:                              [Comp 3] → [Comp 2] → [Comp 1]
//!                     (orden inverso, LIFO)
//!
//! ```
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Errors from compensation operations
#[derive(Debug, Error)]
pub enum CompensationError {
    #[error("No steps to compensate")]
    NoStepsToCompensate,

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),

    #[error("Activity not found: {0}")]
    ActivityNotFound(String),

    #[error("Compensation activity failed: {0}")]
    CompensationActivityFailed(String),
}

/// A completed step that can be compensated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedStep {
    /// Unique step identifier
    pub step_id: String,
    /// Activity type that was executed
    pub activity_type: String,
    /// Compensation activity type
    pub compensation_activity_type: String,
    /// Output from the activity (used as input for compensation)
    pub output: Value,
    /// Input that was provided to the activity
    pub input: Value,
    /// Step order in the workflow
    pub step_order: u32,
    /// When the step was completed
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// Compensation action to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationAction {
    pub step_id: String,
    pub activity_type: String,
    pub compensation_type: String,
    pub input: Value,
    pub retry_count: u32,
    pub max_retries: u32,
}

/// Builder for compensation tracking
#[derive(Debug, Default)]
pub struct CompensationTrackerBuilder {
    steps: Vec<CompletedStep>,
    auto_compensate: bool,
}

impl CompensationTrackerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable automatic compensation on failure
    pub fn with_auto_compensate(mut self, enabled: bool) -> Self {
        self.auto_compensate = enabled;
        self
    }

    /// Build the tracker
    pub fn build(self) -> CompensationTracker {
        CompensationTracker {
            steps: Arc::new(Mutex::new(self.steps)),
            compensating: Arc::new(Mutex::new(false)),
            auto_compensate: self.auto_compensate,
        }
    }
}

/// Tracker for compensation actions
#[derive(Debug, Clone)]
pub struct CompensationTracker {
    /// Completed steps (thread-safe)
    steps: Arc<Mutex<Vec<CompletedStep>>>,
    /// Whether we're currently compensating
    compensating: Arc<Mutex<bool>>,
    /// Enable automatic compensation
    auto_compensate: bool,
}

impl CompensationTracker {
    /// Create a new tracker
    pub fn new() -> Self {
        Self {
            steps: Arc::new(Mutex::new(Vec::new())),
            compensating: Arc::new(Mutex::new(false)),
            auto_compensate: true,
        }
    }

    /// Create a builder
    pub fn builder() -> CompensationTrackerBuilder {
        CompensationTrackerBuilder::new()
    }

    /// Track a completed step with its compensation activity
    ///
    /// # Arguments
    /// * `step_id` - Unique identifier for the step
    /// * `activity_type` - The activity that was executed
    /// * `compensation_activity_type` - Activity to run for compensation
    /// * `input` - Input provided to the activity
    /// * `output` - Output from the activity
    /// * `step_order` - Order of this step in the workflow
    pub fn track_step(
        &self,
        step_id: impl Into<String>,
        activity_type: impl Into<String>,
        compensation_activity_type: impl Into<String>,
        input: Value,
        output: Value,
        step_order: u32,
    ) {
        let step = CompletedStep {
            step_id: step_id.into(),
            activity_type: activity_type.into(),
            compensation_activity_type: compensation_activity_type.into(),
            input,
            output,
            step_order,
            completed_at: chrono::Utc::now(),
        };

        let mut steps = self.steps.lock().unwrap();
        steps.push(step);
    }

    /// Track a step with auto-derived compensation activity
    ///
    /// Compensation activity is derived by prepending "compensate_" to the activity type
    pub fn track_step_with_auto_compensation(
        &self,
        step_id: impl Into<String>,
        activity_type: impl Into<String>,
        input: Value,
        output: Value,
        step_order: u32,
    ) {
        let activity_type_str = activity_type.into();
        let compensation_type = format!("compensate_{}", activity_type_str);
        self.track_step(
            step_id,
            &activity_type_str,
            compensation_type,
            input,
            output,
            step_order,
        );
    }

    /// Get all compensation actions in reverse order (LIFO)
    pub fn get_compensation_actions(&self) -> Vec<CompensationAction> {
        let steps = self.steps.lock().unwrap();

        steps
            .iter()
            .rev()
            .map(|step| CompensationAction {
                step_id: step.step_id.clone(),
                activity_type: step.activity_type.clone(),
                compensation_type: step.compensation_activity_type.clone(),
                input: step.output.clone(), // Output becomes input for compensation
                retry_count: 0,
                max_retries: 3,
            })
            .collect()
    }

    /// Check if we're currently compensating
    pub fn is_compensating(&self) -> bool {
        *self.compensating.lock().unwrap()
    }

    /// Get count of completed steps
    pub fn completed_steps_count(&self) -> usize {
        self.steps.lock().unwrap().len()
    }

    /// Clear all tracked steps (after successful completion)
    pub fn clear(&self) {
        self.steps.lock().unwrap().clear();
    }

    /// Reset the tracker (clear steps and compensating flag)
    pub fn reset(&self) {
        let mut steps = self.steps.lock().unwrap();
        let mut compensating = self.compensating.lock().unwrap();
        steps.clear();
        *compensating = false;
    }

    /// Mark compensation as started
    fn start_compensating(&self) {
        *self.compensating.lock().unwrap() = true;
    }

    /// Mark compensation as completed
    fn stop_compensating(&self) {
        *self.compensating.lock().unwrap() = false;
    }
}

impl Default for CompensationTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for compensation handlers
#[async_trait]
pub trait CompensationHandler: Send + Sync {
    /// Execute compensation for an activity
    async fn compensate(&self, activity_type: &str, input: Value) -> Result<(), CompensationError>;
}

/// In-memory compensation handler for testing
#[derive(Debug, Default)]
pub struct InMemoryCompensationHandler {
    compensations: Arc<Mutex<Vec<CompensationAction>>>,
}

impl InMemoryCompensationHandler {
    pub fn new() -> Self {
        Self {
            compensations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get all compensations that were executed
    pub fn get_compensations(&self) -> Vec<CompensationAction> {
        self.compensations.lock().unwrap().clone()
    }

    /// Clear compensation history
    pub fn clear(&self) {
        self.compensations.lock().unwrap().clear();
    }
}

#[async_trait]
impl CompensationHandler for InMemoryCompensationHandler {
    async fn compensate(&self, activity_type: &str, input: Value) -> Result<(), CompensationError> {
        let action = CompensationAction {
            step_id: format!("compensate-{}", uuid::Uuid::new_v4()),
            activity_type: activity_type.to_string(),
            compensation_type: activity_type.to_string(),
            input,
            retry_count: 0,
            max_retries: 3,
        };

        self.compensations.lock().unwrap().push(action);
        Ok(())
    }
}

/// Workflow error with compensation support
#[derive(Debug, Error)]
pub enum WorkflowError<E: std::error::Error + Send + Sync> {
    #[error("Workflow failed: {0}")]
    Failed(String),

    #[error("Activity error: {0}")]
    ActivityError(#[source] E),

    #[error("Compensation required: {steps} step(s)", steps = .0.len())]
    CompensationRequired(Vec<CompensationAction>),

    #[error("Workflow cancelled: {0}")]
    Cancelled(String),
}

/// Result of workflow execution with compensation support
#[derive(Debug)]
pub struct CompensationWorkflowResult<O, E: std::error::Error + Send + Sync> {
    pub output: Option<O>,
    pub error: Option<WorkflowError<E>>,
    pub compensation_required: bool,
    pub completed_steps: usize,
}

impl<O, E: std::error::Error + Send + Sync> CompensationWorkflowResult<O, E> {
    pub fn completed(output: O, steps: usize) -> Self {
        Self {
            output: Some(output),
            error: None,
            compensation_required: false,
            completed_steps: steps,
        }
    }

    pub fn failed(error: String, steps: usize) -> Self {
        Self {
            output: None,
            error: Some(WorkflowError::Failed(error)),
            compensation_required: steps > 0,
            completed_steps: steps,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_track_step() {
        let tracker = CompensationTracker::new();

        tracker.track_step(
            "step-1",
            "reserve-inventory",
            "release-inventory",
            json!({"product_id": "PROD-001", "quantity": 5}),
            json!({"reservation_id": "RES-001"}),
            1,
        );

        assert_eq!(tracker.completed_steps_count(), 1);
    }

    #[tokio::test]
    async fn test_compensation_order_reverse() {
        let tracker = CompensationTracker::new();

        // Track steps in order 1, 2, 3
        tracker.track_step(
            "step-1",
            "activity-1",
            "compensate-1",
            json!({}),
            json!({}),
            1,
        );
        tracker.track_step(
            "step-2",
            "activity-2",
            "compensate-2",
            json!({}),
            json!({}),
            2,
        );
        tracker.track_step(
            "step-3",
            "activity-3",
            "compensate-3",
            json!({}),
            json!({}),
            3,
        );

        // Get compensation actions
        let actions = tracker.get_compensation_actions();

        // Should be in reverse order: 3, 2, 1
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].compensation_type, "compensate-3");
        assert_eq!(actions[1].compensation_type, "compensate-2");
        assert_eq!(actions[2].compensation_type, "compensate-1");
    }

    #[tokio::test]
    async fn test_auto_derived_compensation() {
        let tracker = CompensationTracker::new();

        tracker.track_step_with_auto_compensation(
            "step-1",
            "process-payment",
            json!({"amount": 100}),
            json!({"transaction_id": "TXN-001"}),
            1,
        );

        let actions = tracker.get_compensation_actions();
        assert_eq!(actions[0].compensation_type, "compensate_process-payment");
    }

    #[tokio::test]
    async fn test_clear_steps() {
        let tracker = CompensationTracker::new();

        tracker.track_step("step-1", "activity", "compensate", json!({}), json!({}), 1);
        assert_eq!(tracker.completed_steps_count(), 1);

        tracker.clear();
        assert_eq!(tracker.completed_steps_count(), 0);
    }

    #[tokio::test]
    async fn test_reset() {
        let tracker = CompensationTracker::new();

        tracker.track_step("step-1", "activity", "compensate", json!({}), json!({}), 1);

        // Simulate compensation started
        tracker.start_compensating();
        assert!(tracker.is_compensating());

        // Reset should clear everything
        tracker.reset();
        assert!(!tracker.is_compensating());
        assert_eq!(tracker.completed_steps_count(), 0);
    }

    #[tokio::test]
    async fn test_in_memory_handler() {
        let handler = InMemoryCompensationHandler::new();

        handler
            .compensate("release-inventory", json!({"reservation_id": "RES-001"}))
            .await
            .unwrap();

        let compensations = handler.get_compensations();
        assert_eq!(compensations.len(), 1);
        assert_eq!(compensations[0].activity_type, "release-inventory");
    }
}

// Helper for json! macro
fn json_value<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}
