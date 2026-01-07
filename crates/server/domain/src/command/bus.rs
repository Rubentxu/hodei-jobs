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
    pub registry: Arc<parking_lot::Mutex<HandlerRegistry>>,
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
        let registry = Arc::new(parking_lot::Mutex::new(HandlerRegistry::new()));
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
        let registry = Arc::new(parking_lot::Mutex::new(HandlerRegistry::new()));
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
        if !key.is_empty() {
            let key_str = key.as_ref();
            let idempotency = self.idempotency.lock().await;
            if idempotency.is_duplicate(key_str).await {
                return Err(CommandError::IdempotencyConflict {
                    key: key_str.to_string(),
                });
            }
        }
        Err(CommandError::HandlerNotFound {
            command_type: std::any::type_name_of_val(&command),
        })
    }

    async fn register_handler<H, C>(&mut self, _handler: H)
    where
        H: CommandHandler<C>,
        C: Command,
    {
    }
}

/// In-memory idempotency checker.
#[derive(Debug, Default)]
pub struct InMemoryIdempotencyChecker {
    processed: Arc<parking_lot::Mutex<std::collections::HashSet<String>>>,
}

impl InMemoryIdempotencyChecker {
    pub fn new() -> Self {
        Self {
            processed: Arc::new(parking_lot::Mutex::new(std::collections::HashSet::new())),
        }
    }

    pub async fn is_duplicate(&self, key: &str) -> bool {
        self.processed.lock().contains(key)
    }

    pub async fn mark_processed(&self, key: &str) {
        self.processed.lock().insert(key.to_string());
    }

    pub async fn clear(&self) {
        self.processed.lock().clear();
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
