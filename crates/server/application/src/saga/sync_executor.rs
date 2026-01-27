#![allow(deprecated)]

//!
//! # Synchronous Workflow Executor
//!
//! A simple synchronous executor for saga workflows.
//! This is used for testing and simple execution scenarios.

use saga_engine_core::workflow::{
    DynWorkflowStep, StepResult, WorkflowConfig, WorkflowContext, WorkflowDefinition,
    WorkflowResult,
};
use serde_json::Value;

fn get_step_type_id(_step: &Box<dyn DynWorkflowStep>) -> String {
    let type_id = std::any::type_name::<dyn DynWorkflowStep>();
    type_id.to_string()
}

/// Simple synchronous workflow executor
pub struct SyncWorkflowExecutor<W: WorkflowDefinition> {
    workflow: W,
}

impl<W: WorkflowDefinition> SyncWorkflowExecutor<W> {
    pub fn new(workflow: W) -> Self {
        Self { workflow }
    }

    pub async fn execute(&self, input: Value) -> Result<WorkflowResult, String> {
        let config = self.workflow.configuration();
        let mut context = WorkflowContext::new(
            saga_engine_core::event::SagaId::new(),
            saga_engine_core::workflow::WorkflowTypeId::new::<W>(),
            config,
        );

        let mut step_outputs: Vec<Value> = Vec::new();

        for (step_index, step) in self.workflow.steps().iter().enumerate() {
            let step_type = get_step_type_id(step);

            match step.execute(&mut context, input.clone()).await {
                Ok(result) => match result {
                    StepResult::Output(output) => {
                        step_outputs.push(output.clone());
                        context.set_step_output(format!("step_{}", step_index), output);
                    }
                    StepResult::Waiting { signal_type, .. } => {
                        return Ok(WorkflowResult::Failed {
                            error: format!("Step is waiting for signal: {}", signal_type),
                            failed_step: Some(step_type),
                            steps_executed: step_index,
                            compensations_executed: 0,
                        });
                    }
                    StepResult::Retry { error, delay_ms: _ } => {
                        return Ok(WorkflowResult::Failed {
                            error: format!("Step requested retry: {}", error),
                            failed_step: Some(step_type),
                            steps_executed: step_index,
                            compensations_executed: 0,
                        });
                    }
                    StepResult::SideEffect { description: _ } => {
                        step_outputs.push(Value::Null);
                    }
                },
                Err(e) => {
                    let failed_step = e.step_type.clone();

                    let mut compensations = 0;
                    for (_comp_index, output) in step_outputs.iter().enumerate().rev() {
                        if let Err(_comp_err) = step.compensate(&mut context, output.clone()).await
                        {
                            eprintln!("Compensation failed");
                        } else {
                            compensations += 1;
                        }
                    }

                    return Ok(WorkflowResult::Failed {
                        error: e.to_string(),
                        failed_step: Some(failed_step),
                        steps_executed: step_index,
                        compensations_executed: compensations,
                    });
                }
            }

            context.advance_step();
        }

        let final_output = step_outputs.last().cloned().unwrap_or(Value::Null);

        Ok(WorkflowResult::Completed {
            output: final_output,
            steps_executed: self.workflow.steps().len(),
            duration: chrono::Duration::zero(),
        })
    }
}
