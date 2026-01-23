use crate::workflow::{WorkflowContext, durable::DurableWorkflow};
use async_trait::async_trait;
use dashmap::DashMap;
use std::fmt::Debug;
use std::sync::Arc;

/// Type-erased durable workflow trait for storage in registry.
#[async_trait]
pub trait DynDurableWorkflow: Send + Sync + Debug {
    /// Run the workflow with type-erased input.
    async fn run_dyn(
        &self,
        ctx: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>;

    /// Get version of the workflow.
    fn version(&self) -> u32;
}

#[async_trait]
impl<W: DurableWorkflow + Debug> DynDurableWorkflow for W {
    async fn run_dyn(
        &self,
        ctx: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        // Deserialize input
        let typed_input: W::Input = serde_json::from_value(input)
            .map_err(|e| format!("Invalid input for {}: {}", W::TYPE_ID, e))?;

        // Run workflow
        let output = self
            .run(ctx, typed_input)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Serialize output
        serde_json::to_value(output)
            .map_err(|e| format!("Failed to serialize output for {}: {}", W::TYPE_ID, e).into())
    }

    fn version(&self) -> u32 {
        W::VERSION
    }
}

/// Registry for durable workflows.
#[derive(Debug, Default)]
pub struct WorkflowRegistry {
    workflows: DashMap<String, Arc<dyn DynDurableWorkflow>>,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self {
            workflows: DashMap::new(),
        }
    }

    /// Register a durable workflow.
    pub fn register_workflow<W: DurableWorkflow + Debug>(&self, workflow: W) {
        let type_id = W::TYPE_ID.to_string();
        self.workflows.insert(type_id, Arc::new(workflow));
    }

    /// Register a durable workflow with custom name alias.
    pub fn register_workflow_with_name<W: DurableWorkflow + Debug>(&self, name: &str, workflow: W) {
        self.workflows.insert(name.to_string(), Arc::new(workflow));
    }

    /// Get a workflow by type ID.
    pub fn get_workflow(&self, type_id: &str) -> Option<Arc<dyn DynDurableWorkflow>> {
        self.workflows.get(type_id).map(|r| r.value().clone())
    }

    /// Check if workflow is registered.
    pub fn has_workflow(&self, type_id: &str) -> bool {
        self.workflows.contains_key(type_id)
    }

    /// Get number of registered workflows.
    pub fn len(&self) -> usize {
        self.workflows.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.workflows.is_empty()
    }

    /// Get all registered workflow type IDs.
    pub fn workflow_keys(&self) -> Vec<String> {
        self.workflows
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::durable::DurableWorkflow;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone)]
    struct TestWorkflow;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct TestInput(String);

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct TestOutput(String);

    #[derive(Debug, thiserror::Error)]
    #[error("Test error")]
    struct TestError;

    #[async_trait]
    impl DurableWorkflow for TestWorkflow {
        const TYPE_ID: &'static str = "test-workflow";
        type Input = TestInput;
        type Output = TestOutput;
        type Error = TestError;

        async fn run(
            &self,
            _ctx: &mut WorkflowContext,
            input: Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(TestOutput(input.0))
        }
    }

    #[test]
    fn test_workflow_registration() {
        let registry = WorkflowRegistry::new();
        registry.register_workflow(TestWorkflow);

        assert!(registry.has_workflow("test-workflow"));
        assert_eq!(registry.len(), 1);
    }
}
