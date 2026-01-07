// Command Bus Tests
//
// Integration tests for the Command Bus infrastructure.
// These tests verify the core functionality of the Command pattern implementation.

use crate::command::{
    Command, CommandBus, CommandError, CommandHandler, CommandMetadataDefault, CommandResult,
    HandlerRegistry, InMemoryCommandBus, LoggingLayer, RetryLayer, TelemetryLayer,
};
use std::borrow::Cow;
use std::sync::Arc;
use tokio::time::Duration;

// === Test Commands ===

#[derive(Debug)]
struct TestCommand {
    value: String,
}

impl Command for TestCommand {
    type Output = String;
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(self.value.clone())
    }
}

#[derive(Debug)]
struct TestCommandNoKey;

impl Command for TestCommandNoKey {
    type Output = ();
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
}

#[derive(Debug)]
struct SlowCommand;

impl Command for SlowCommand {
    type Output = String;
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Borrowed("slow-command")
    }
}

#[derive(Debug)]
struct FailingCommand;

impl Command for FailingCommand {
    type Output = ();
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Borrowed("failing-command")
    }
}

// === Test Handlers ===

#[derive(Debug)]
struct TestHandler;

#[async_trait::async_trait]
impl CommandHandler<TestCommand> for TestHandler {
    type Error = CommandError;
    async fn handle(&self, cmd: TestCommand) -> Result<String, CommandError> {
        Ok(format!("handled: {}", cmd.value))
    }
}

#[derive(Debug)]
struct TestHandlerNoKey;

#[async_trait::async_trait]
impl CommandHandler<TestCommandNoKey> for TestHandlerNoKey {
    type Error = CommandError;
    async fn handle(&self, _: TestCommandNoKey) -> Result<(), CommandError> {
        Ok(())
    }
}

#[derive(Debug)]
struct SlowHandler;

#[async_trait::async_trait]
impl CommandHandler<SlowCommand> for SlowHandler {
    type Error = CommandError;
    async fn handle(&self, _: SlowCommand) -> Result<String, CommandError> {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok("slow result".to_string())
    }
}

#[derive(Debug)]
struct FailingHandler(u32);

#[async_trait::async_trait]
impl CommandHandler<FailingCommand> for FailingHandler {
    type Error = CommandError;
    async fn handle(&self, _: FailingCommand) -> Result<(), CommandError> {
        if self.0 > 0 {
            Err(CommandError::ExecutionFailed {
                source: anyhow::anyhow!("transient error"),
            })
        } else {
            Ok(())
        }
    }
}

// === Tests ===

#[tokio::test]
async fn test_dispatch_success() {
    let mut bus = InMemoryCommandBus::new_test();
    let handler = Arc::new(TestHandler);
    bus.register_handler::<TestHandler, TestCommand>(handler.clone())
        .await;

    let cmd = TestCommand {
        value: "test-123".to_string(),
    };
    let result = bus.dispatch(cmd).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "handled: test-123");
}

#[tokio::test]
async fn test_dispatch_handler_not_found() {
    let bus = InMemoryCommandBus::new_test();
    let cmd = TestCommand {
        value: "test".to_string(),
    };
    let result = bus.dispatch(cmd).await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CommandError::HandlerNotFound { .. }
    ));
}

#[tokio::test]
async fn test_dispatch_idempotency() {
    let mut bus = InMemoryCommandBus::new_test();
    let handler = Arc::new(TestHandler);
    bus.register_handler::<TestHandler, TestCommand>(handler.clone())
        .await;

    let cmd = TestCommand {
        value: "idempotent-key".to_string(),
    };

    // First dispatch should succeed
    let result1 = bus.dispatch(cmd.clone()).await;
    assert!(result1.is_ok());

    // Second dispatch with same key should fail
    let result2 = bus.dispatch(cmd).await;
    assert!(result2.is_err());
    assert!(matches!(
        result2.unwrap_err(),
        CommandError::IdempotencyConflict { .. }
    ));
}

#[tokio::test]
async fn test_dispatch_failure() {
    let mut bus = InMemoryCommandBus::new_test();
    let handler = Arc::new(FailingHandler(1));
    bus.register_handler::<FailingHandler, FailingCommand>(handler)
        .await;

    let cmd = FailingCommand;
    let result = bus.dispatch(cmd).await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CommandError::ExecutionFailed { .. }
    ));
}

#[tokio::test]
async fn test_idempotency_checker() {
    let checker = crate::command::bus::InMemoryIdempotencyChecker::new();

    assert!(!checker.is_duplicate("key-1").await);

    checker.mark_processed("key-1").await;
    assert!(checker.is_duplicate("key-1").await);

    assert!(!checker.is_duplicate("key-2").await);
}

#[tokio::test]
async fn test_idempotency_checker_clear() {
    let checker = crate::command::bus::InMemoryIdempotencyChecker::new();
    checker.mark_processed("key-1").await;
    assert!(checker.is_duplicate("key-1").await);

    checker.clear().await;
    assert!(!checker.is_duplicate("key-1").await);
}

#[tokio::test]
async fn test_logging_layer() {
    let bus = InMemoryCommandBus::new_test();
    let logging_bus = LoggingLayer::new().layer(bus);

    let result = logging_bus.dispatch(TestCommandNoKey).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_retry_layer_no_retry_on_success() {
    let bus = InMemoryCommandBus::new_test();
    let handler = Arc::new(TestHandlerNoKey);
    bus.register_handler::<TestHandlerNoKey, TestCommandNoKey>(handler)
        .await;

    let retry_bus = RetryLayer::default().layer(bus);
    let result = retry_bus.dispatch(TestCommandNoKey).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_retry_layer_retries() {
    let mut bus = InMemoryCommandBus::new_test();
    let handler = Arc::new(FailingHandler(1)); // Fails once, then succeeds
    bus.register_handler::<FailingHandler, FailingCommand>(handler)
        .await;

    let retry_config = RetryLayer::new().config;
    let retry_bus = RetryLayer::with_config(retry_config).layer(bus);

    let result = retry_bus.dispatch(FailingCommand).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_retry_layer_exhausts_retries() {
    let mut bus = InMemoryCommandBus::new_test();
    let handler = Arc::new(FailingHandler(100)); // Always fails
    bus.register_handler::<FailingHandler, FailingCommand>(handler)
        .await;

    let retry_config = RetryLayer::new().config;
    let retry_bus = RetryLayer::with_config(retry_config).layer(bus);

    let result = retry_bus.dispatch(FailingCommand).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_telemetry_layer_records_duration() {
    let mut bus = InMemoryCommandBus::new_test();
    let handler = Arc::new(SlowHandler);
    bus.register_handler::<SlowHandler, SlowCommand>(handler.clone())
        .await;

    let telemetry_bus = TelemetryLayer::new().layer(bus);

    let start = tokio::time::Instant::now();
    let result = telemetry_bus.dispatch(SlowCommand).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    assert!(elapsed >= Duration::from_millis(50));
}

// === Registry Tests ===

#[tokio::test]
fn test_registry_registers_and_retrieves() {
    let mut registry = HandlerRegistry::new();
    let handler = Arc::new(TestHandler);

    registry.register::<TestCommand, TestHandler>(handler.clone());

    let retrieved = registry.get_handler::<TestCommand>();
    assert!(retrieved.is_some());
    assert!(Arc::ptr_eq(&retrieved.unwrap(), &handler));
}

#[tokio::test]
fn test_registry_returns_none_for_unknown() {
    let registry = HandlerRegistry::new();

    let retrieved = registry.get_handler::<TestCommand>();
    assert!(retrieved.is_none());
}

#[tokio::test]
fn test_registry_has_handler() {
    let mut registry = HandlerRegistry::new();
    assert!(!registry.has_handler::<TestCommand>());

    let handler = Arc::new(TestHandler);
    registry.register::<TestCommand, TestHandler>(handler);

    assert!(registry.has_handler::<TestCommand>());
}

#[tokio::test]
fn test_registry_len() {
    let mut registry = HandlerRegistry::new();
    assert_eq!(registry.len(), 0);

    let handler = Arc::new(TestHandler);
    registry.register::<TestCommand, TestHandler>(handler);

    assert_eq!(registry.len(), 1);
}

// === Command Metadata Tests ===

#[tokio::test]
fn test_command_metadata_new() {
    let metadata = CommandMetadataDefault::new();
    assert!(metadata.trace_id().is_some());
    assert!(metadata.created_at().is_some());
    assert!(metadata.saga_id().is_none());
}

#[tokio::test]
fn test_command_metadata_with_saga_id() {
    let metadata = CommandMetadataDefault::new().with_saga_id("saga-123");
    assert_eq!(metadata.saga_id(), Some("saga-123"));
}

#[tokio::test]
fn test_command_metadata_clone() {
    let metadata = CommandMetadataDefault::new()
        .with_saga_id("saga-123")
        .with_issuer("user-1");
    let cloned = metadata.clone();
    assert_eq!(cloned.saga_id(), metadata.saga_id());
}

// === Command Error Tests ===

#[tokio::test]
fn test_command_error_types() {
    let err = CommandError::HandlerNotFound {
        command_type: "TestCommand",
    };
    assert_eq!(err.command_type(), Some("TestCommand"));

    let err = CommandError::IdempotencyConflict {
        key: "key-123".to_string(),
    };
    assert!(err.to_string().contains("key-123"));
}

#[tokio::test]
fn test_command_error_from_anyhow() {
    let anyhow_err = anyhow::anyhow!("test error");
    let cmd_err: CommandError = anyhow_err.into();
    assert!(matches!(cmd_err, CommandError::ExecutionFailed { .. }));
}
