//!
//! # Synchronous Durable Workflow Executor
//!
//! A simple synchronous executor for DurableWorkflow (Workflow-as-Code).
//! This is used for testing and simple execution scenarios where activities
//! complete immediately.

use saga_engine_core::workflow::{
    DurableWorkflow, WorkflowConfig, WorkflowContext, WorkflowResult,
};
use serde_json::Value;
use std::fmt::Debug;

/// Simple synchronous executor for DurableWorkflow
///
/// This executor assumes all activities complete synchronously and returns
/// their results immediately. It's suitable for testing and simple scenarios.
///
/// For production use, use the saga-engine v4.0 DurableWorkflowRuntime with
/// proper task queues and event sourcing.
pub struct SyncDurableWorkflowExecutor<W: DurableWorkflow> {
    workflow: W,
    config: WorkflowConfig,
}

impl<W: DurableWorkflow> SyncDurableWorkflowExecutor<W> {
    pub fn new(workflow: W) -> Self {
        let config = WorkflowConfig {
            default_timeout_secs: 300,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: W::TYPE_ID.to_string(),
        };

        Self { workflow, config }
    }

    pub async fn execute(&self, input: Value) -> Result<WorkflowResult, String> {
        let execution_id = saga_engine_core::event::SagaId::new();
        let workflow_type = saga_engine_core::workflow::WorkflowTypeId::new::<W>();

        let mut context = WorkflowContext::new(execution_id, workflow_type, self.config.clone());

        // Convert input to type-safe input
        let workflow_input: W::Input = serde_json::from_value(input)
            .map_err(|e| format!("Failed to deserialize workflow input: {}", e))?;

        // Execute the workflow
        match self.workflow.run(&mut context, workflow_input).await {
            Ok(output) => {
                let output_value = serde_json::to_value(&output)
                    .map_err(|e| format!("Failed to serialize workflow output: {}", e))?;

                let duration = context.duration_so_far();

                Ok(WorkflowResult::Completed {
                    output: output_value,
                    steps_executed: context.current_step_index,
                    duration,
                })
            }
            Err(e) => {
                // Check if it's a pause signal (workflow is waiting for activity)
                // For sync executor, we treat this as an error since we can't actually pause
                Ok(WorkflowResult::Failed {
                    error: e.to_string(),
                    failed_step: Some(format!("{}.run", W::TYPE_ID)),
                    steps_executed: context.current_step_index,
                    compensations_executed: 0,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::workflow::DurableWorkflow;

    #[derive(Debug)]
    struct TestWorkflow;

    #[async_trait::async_trait]
    impl DurableWorkflow for TestWorkflow {
        const TYPE_ID: &'static str = "test";
        const VERSION: u32 = 1;

        type Input = serde_json::Value;
        type Output = serde_json::Value;
        type Error = std::io::Error;

        async fn run(
            &self,
            _ctx: &mut WorkflowContext,
            input: Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn test_sync_durable_workflow_executor() {
        let workflow = TestWorkflow;
        let executor = SyncDurableWorkflowExecutor::new(workflow);

        let input = serde_json::json!({"test": "value"});
        let result = executor.execute(input).await.unwrap();

        match result {
            WorkflowResult::Completed { output, .. } => {
                assert_eq!(output, serde_json::json!({"test": "value"}));
            }
            _ => panic!("Expected completed result"),
        }
    }
}
