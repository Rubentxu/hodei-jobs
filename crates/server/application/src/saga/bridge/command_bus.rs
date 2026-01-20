//!
//! # CommandBus Activity Bridge (Simplified)
//!
//! Basic bridge between CommandBus and Activity pattern.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

use hodei_server_domain::command::{Command, CommandHandler, DynCommandBus};

/// Error types for CommandBus Activity Bridge
#[derive(Debug, thiserror::Error)]
pub enum CommandBusActivityError<E: Debug + Send + Sync> {
    #[error("Command execution failed: {source}")]
    CommandFailed {
        command_name: &'static str,
        source: E,
    },
    #[error("Failed to serialize: {source}")]
    SerializationFailed { source: serde_json::Error },
    #[error("Activity {activity_type} already exists")]
    ActivityAlreadyExists { activity_type: &'static str },
}

/// Input type for activities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityInput<I: Debug + Clone> {
    pub input: I,
    pub saga_id: Option<String>,
}

/// Output type for activities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityOutput<O: Debug + Clone> {
    pub output: O,
    pub activity_type: &'static str,
}

impl<I: Debug + Clone> ActivityInput<I> {
    pub fn new(input: I, saga_id: Option<String>) -> Self {
        Self { input, saga_id }
    }
}

impl<O: Debug + Clone> ActivityOutput<O> {
    pub fn new(output: O, activity_type: &'static str) -> Self {
        Self {
            output,
            activity_type,
        }
    }
}

/// Activity trait for saga-engine compatibility.
#[async_trait]
pub trait Activity: Send + Sync {
    type Input: Send + Clone;
    type Output: Send + Clone;
    type Error: Debug + Send + Clone;

    fn activity_type_id(&self) -> &'static str;
    fn task_queue(&self) -> Option<&str>;
    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct CommandBusActivityConfig {
    pub default_task_queue: String,
    pub enable_caching: bool,
    pub cache_ttl_seconds: u64,
}

impl Default for CommandBusActivityConfig {
    fn default() -> Self {
        Self {
            default_task_queue: "default".to_string(),
            enable_caching: false,
            cache_ttl_seconds: 300,
        }
    }
}

#[derive(Debug, Default)]
pub struct CommandBusActivityRegistry {
    activities: std::collections::HashMap<&'static str, CommandBusActivityConfig>,
}

impl CommandBusActivityRegistry {
    pub fn new() -> Self {
        Self {
            activities: std::collections::HashMap::new(),
        }
    }

    pub fn register(
        &mut self,
        activity_type: &'static str,
        config: CommandBusActivityConfig,
    ) -> Result<(), CommandBusActivityError<()>> {
        if self.activities.contains_key(activity_type) {
            return Err(CommandBusActivityError::ActivityAlreadyExists { activity_type });
        }
        self.activities.insert(activity_type, config);
        Ok(())
    }

    pub fn get(&self, activity_type: &str) -> Option<&CommandBusActivityConfig> {
        self.activities.get(activity_type)
    }

    pub fn contains(&self, activity_type: &str) -> bool {
        self.activities.contains_key(activity_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hodei_server_domain::command::{Command, CommandHandler};
    use std::borrow::Cow;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestCommand {
        pub value: i32,
        pub saga_id: String,
    }

    impl Command for TestCommand {
        type Output = i32;
        fn idempotency_key(&self) -> Cow<'_, str> {
            Cow::Owned(format!("test-{}", self.value))
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestInput {
        pub value: i32,
    }

    #[derive(Debug, thiserror::Error)]
    #[error("Test error: {value}")]
    struct TestError {
        value: String,
    }

    struct TestHandler;

    #[async_trait]
    impl CommandHandler<TestCommand> for TestHandler {
        type Error = TestError;
        async fn handle(&self, command: TestCommand) -> Result<i32, Self::Error> {
            Ok(command.value * 2)
        }
    }

    #[tokio::test]
    async fn test_activity_input_creation() {
        let input = ActivityInput::new(TestInput { value: 42 }, Some("test-saga".to_string()));
        assert_eq!(input.input.value, 42);
        assert_eq!(input.saga_id, Some("test-saga".to_string()));
    }

    #[tokio::test]
    async fn test_activity_output_creation() {
        let output = ActivityOutput::new(84, "test-activity");
        assert_eq!(output.output, 84);
        assert_eq!(output.activity_type, "test-activity");
    }

    #[tokio::test]
    async fn test_activity_registry() {
        let mut registry = CommandBusActivityRegistry::new();
        let config = CommandBusActivityConfig::default();
        let result = registry.register("test-activity", config);
        assert!(result.is_ok());
        assert!(registry.contains("test-activity"));
    }
}
