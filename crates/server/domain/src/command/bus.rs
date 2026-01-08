// InMemoryCommandBus - Async in-memory implementation

use super::*;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::Duration;

/// Configuration for the command bus.
#[derive(Debug, Clone)]
pub struct CommandBusConfig {
    pub queue_depth: usize,
    pub default_timeout: Duration,
    pub spawn_background: bool,
}

impl Default for CommandBusConfig {
    fn default() -> Self {
        Self {
            queue_depth: 1000,
            default_timeout: Duration::from_secs(30),
            spawn_background: true,
        }
    }
}

enum CommandMessage {
    Shutdown,
}

/// In-memory command bus implementation.
#[derive(Debug)]
pub struct InMemoryCommandBus {
    pub registry: Arc<Mutex<HandlerRegistry>>,
    tx: mpsc::Sender<CommandMessage>,
    pub idempotency: Arc<Mutex<InMemoryIdempotencyChecker>>,
    shutdown_rx: Option<oneshot::Receiver<()>>,
}

impl InMemoryCommandBus {
    pub fn new() -> Self {
        Self::with_config(CommandBusConfig::default())
    }

    pub fn with_config(config: CommandBusConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.queue_depth);
        let registry = Arc::new(Mutex::new(HandlerRegistry::new()));
        let idempotency = Arc::new(Mutex::new(InMemoryIdempotencyChecker::new()));
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            let mut rx = rx;
            let mut shutdown_tx = shutdown_tx;
            loop {
                tokio::select! {
                    Some(msg) = rx.recv() => {
                        if let CommandMessage::Shutdown = msg {
                            break;
                        }
                    }
                    _ = shutdown_tx.closed() => {
                        break;
                    }
                }
            }
        });

        Self {
            registry,
            tx,
            idempotency,
            shutdown_rx: Some(shutdown_rx),
        }
    }

    #[cfg(test)]
    pub fn new_test() -> Self {
        let (tx, _rx) = mpsc::channel(100);
        let registry = Arc::new(Mutex::new(HandlerRegistry::new()));
        let idempotency = Arc::new(Mutex::new(InMemoryIdempotencyChecker::new()));

        Self {
            registry,
            tx,
            idempotency,
            shutdown_rx: None,
        }
    }
}

impl Drop for InMemoryCommandBus {
    fn drop(&mut self) {
        let _ = self.tx.try_send(CommandMessage::Shutdown);
    }
}

#[async_trait]
impl CommandBus for InMemoryCommandBus {
    async fn dispatch<C: Command>(&self, command: C) -> CommandResult<C::Output> {
        let key = command.idempotency_key();
        let key_str = key.as_ref();

        // Check idempotency if key is provided
        if !key.is_empty() {
            let idempotency = self.idempotency.lock().await;
            if idempotency.is_duplicate(key_str).await {
                return Err(CommandError::IdempotencyConflict {
                    key: key_str.to_string(),
                });
            }
        }

        // Get handler from registry
        let registry = self.registry.lock().await;
        let handler_box = registry.get_handler::<C>().ok_or_else(|| {
            CommandError::HandlerNotFound {
                command_type: std::any::type_name::<C>().to_string(),
            }
        })?;

        // Downcast handler to correct type
        use std::any::Any;
        let handler_any: &dyn Any = &**handler_box;
        let handler: &Arc<dyn CommandHandler<C, Error = anyhow::Error>> = handler_any
            .downcast_ref::<Arc<dyn CommandHandler<C, Error = anyhow::Error>>>()
            .ok_or_else(|| CommandError::HandlerNotFound {
                command_type: std::any::type_name::<C>().to_string(),
            })?;

        // Execute handler (clone key for use after handler execution)
        let key_for_marking = if key.is_empty() { None } else { Some(key_str.to_string()) };
        let result = handler.handle(command).await;

        // Mark as processed if idempotency key was provided
        if let Some(key_to_mark) = key_for_marking {
            let mut idempotency = self.idempotency.lock().await;
            idempotency.mark_processed(&key_to_mark).await;
        }

        result.map_err(|e| CommandError::HandlerError {
            command_type: std::any::type_name::<C>().to_string(),
            error: format!("{:?}", e),
        })
    }

    async fn register_handler<H, C>(&mut self, handler: H)
    where
        H: CommandHandler<C> + Send + Sync + 'static,
        C: Command,
    {
        let mut registry = self.registry.lock().await;
        registry.insert::<C, H>(handler);
    }
}

/// In-memory idempotency checker.
#[derive(Debug, Default)]
pub struct InMemoryIdempotencyChecker {
    processed: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl InMemoryIdempotencyChecker {
    pub fn new() -> Self {
        Self {
            processed: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }

    pub async fn is_duplicate(&self, key: &str) -> bool {
        self.processed.lock().await.contains(key)
    }

    pub async fn mark_processed(&self, key: &str) {
        self.processed.lock().await.insert(key.to_string());
    }

    pub async fn clear(&self) {
        self.processed.lock().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[derive(Debug, Clone)]
    struct TestCommand {
        value: String,
    }

    impl Command for TestCommand {
        type Output = String;
        fn idempotency_key(&self) -> Cow<'_, str> {
            Cow::Owned(self.value.clone())
        }
    }

    #[tokio::test]
    async fn test_idempotency_checker() {
        let checker = InMemoryIdempotencyChecker::new();
        assert!(!checker.is_duplicate("key-1").await);
        checker.mark_processed("key-1").await;
        assert!(checker.is_duplicate("key-1").await);
    }

    #[tokio::test]
    async fn test_idempotency_checker_clear() {
        let checker = InMemoryIdempotencyChecker::new();
        checker.mark_processed("key-1").await;
        assert!(checker.is_duplicate("key-1").await);
        checker.clear().await;
        assert!(!checker.is_duplicate("key-1").await);
    }

    #[tokio::test]
    async fn test_handler_not_found() {
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
}
