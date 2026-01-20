/// # CommandBus Activity Bridge (US-94.14)
///
/// Bridge between CommandBus and Activity pattern for saga-engine v4.0.
///
/// This module provides an adapter that wraps CommandBus implementations
/// as Activities, enabling seamless integration between the legacy command system
/// and saga-engine v4.0 workflow activities.
use async_trait::async_trait;
use hodei_server_domain::command::erased::dispatch_erased;
use hodei_server_domain::command::{Command, CommandError, CommandHandler, DynCommandBus};
use saga_engine_core::workflow::Activity;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::sync::Arc;

/// Error types for CommandBus Activity Bridge
#[derive(Debug, thiserror::Error)]
pub enum CommandBusActivityError {
    #[error("Command execution failed: {source}")]
    CommandExecutionFailed { source: CommandError },
    #[error("Activity {activity_type} already exists")]
    ActivityAlreadyExists { activity_type: &'static str },
}

/// Bridge activity that wraps a CommandBus implementation.
///
/// This activity allows commands to be executed as activities within
/// saga-engine v4.0 workflows, maintaining backward compatibility
/// with the existing command system.
pub struct CommandBusActivity<C: Command> {
    /// The command bus for dispatching commands
    command_bus: Arc<DynCommandBus>,
    /// Marker for command type
    _phantom: PhantomData<C>,
}

impl<C: Command> Debug for CommandBusActivity<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandBusActivity").finish()
    }
}

impl<C: Command> CommandBusActivity<C> {
    /// Create a new CommandBusActivity
    pub fn new(command_bus: Arc<DynCommandBus>) -> Self {
        Self {
            command_bus,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<C> Activity for CommandBusActivity<C>
where
    C: Command + Serialize + for<'de> Deserialize<'de> + Send + Clone + 'static,
    C::Output: Serialize + for<'de> Deserialize<'de> + Clone + Send,
{
    const TYPE_ID: &'static str = "command-bus-activity";

    type Input = C;
    type Output = C::Output;
    type Error = CommandBusActivityError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // Dispatch via command bus using the helper function
        dispatch_erased(&self.command_bus, input)
            .await
            .map_err(|e| CommandBusActivityError::CommandExecutionFailed { source: e })
    }
}

/// Activity registry for command bus activities
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

    pub fn register<C>(
        &mut self,
        activity_type: &'static str,
        _config: CommandBusActivityConfig,
    ) -> Result<(), CommandBusActivityError>
    where
        C: Command + Serialize + for<'de> Deserialize<'de>,
    {
        if self.activities.contains_key(activity_type) {
            return Err(CommandBusActivityError::ActivityAlreadyExists { activity_type });
        }
        self.activities.insert(activity_type, _config);
        Ok(())
    }

    pub fn get(&self, activity_type: &str) -> Option<&CommandBusActivityConfig> {
        self.activities.get(activity_type)
    }

    pub fn contains(&self, activity_type: &str) -> bool {
        self.activities.contains_key(activity_type)
    }

    /// Get all registered activity types
    pub fn activity_types(&self) -> Vec<&'static str> {
        self.activities.keys().copied().collect()
    }
}

/// Configuration for CommandBusActivity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandBusActivityConfig {
    /// Default task queue for this activity
    pub default_task_queue: String,
    /// Enable command caching
    pub enable_caching: bool,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Retry configuration
    pub retry_config: RetryConfig,
}

impl Default for CommandBusActivityConfig {
    fn default() -> Self {
        Self {
            default_task_queue: "default".to_string(),
            enable_caching: false,
            cache_ttl_seconds: 300,
            retry_config: RetryConfig::default(),
        }
    }
}

/// Retry configuration for command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: u32,
    /// Base delay in milliseconds
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Jitter factor (0.0 to 1.0)
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            jitter: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::command::{CommandBus, InMemoryCommandBus};

    // Test command - must be serde-compatible
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestCommand {
        pub value: i32,
    }

    impl Command for TestCommand {
        type Output = i32;

        fn idempotency_key(&self) -> std::borrow::Cow<'_, str> {
            std::borrow::Cow::Owned(format!("test-{}", self.value))
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("Test error: {value}")]
    struct TestError {
        value: String,
    }

    struct TestHandler;

    #[async_trait::async_trait]
    impl hodei_server_domain::command::CommandHandler<TestCommand> for TestHandler {
        type Error = TestError;
        async fn handle(&self, command: TestCommand) -> Result<i32, Self::Error> {
            Ok(command.value * 2)
        }
    }

    #[tokio::test]
    async fn test_activity_registry() {
        let mut registry = CommandBusActivityRegistry::new();
        let config = CommandBusActivityConfig::default();
        let result = registry.register::<TestCommand>("test-activity", config);
        assert!(result.is_ok());
        assert!(registry.contains("test-activity"));
    }
}
