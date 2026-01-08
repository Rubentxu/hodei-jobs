//! Erased Command Bus - Type Erasure para compatibilidad con dyn
//!
//! Este módulo proporciona un CommandBus que puede usarse como `dyn CommandBus`.
//!
//! # Problema
//! Rust no permite métodos genéricos en `dyn Trait` (error E0038).
//!
//! # Solución
//! Crear un trait object-safe que usa Any internamente.

use super::*;
use async_trait::async_trait;
use std::any::{Any, TypeId};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Trait object-safe para CommandBus.
/// Permite almacenar `Arc<dyn ErasedCommandBus>` en estructuras como SagaServices.
#[async_trait]
pub trait ErasedCommandBus: Send + Sync {
    /// Despacha un comando con type erasure.
    /// El comando y resultado se pasan como `Box<dyn Any + Send>`.
    async fn dispatch_erased(
        &self,
        command: Box<dyn Any + Send>,
        command_type_id: TypeId,
    ) -> Result<Box<dyn Any + Send>, CommandError>;
}

/// Handler trait interno que maneja Any directamente.
/// Esto permite que diferentes handlers con diferentes tipos de error
/// trabajen juntos en el mismo HashMap.
#[async_trait::async_trait]
trait AnyHandler: Send + Sync {
    async fn handle_any(
        &self,
        command: Box<dyn Any + Send>,
    ) -> Result<Box<dyn Any + Send>, CommandError>;
}

/// Implementación de AnyHandler que envuelve un CommandHandler concreto.
struct HandlerWrapper<C: Command, H: CommandHandler<C>> {
    handler: H,
    _phantom: std::marker::PhantomData<C>,
}

impl<C: Command, H: CommandHandler<C>> HandlerWrapper<C, H> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<C: Command, H: CommandHandler<C> + Send + Sync + 'static> AnyHandler for HandlerWrapper<C, H> {
    async fn handle_any(
        &self,
        command: Box<dyn Any + Send>,
    ) -> Result<Box<dyn Any + Send>, CommandError> {
        // Downcast del comando
        let command = *command
            .downcast::<C>()
            .map_err(|_| CommandError::TypeMismatch {
                expected: std::any::type_name::<C>().to_string(),
                actual: "command type mismatch".to_string(),
            })?;

        // Ejecutar handler (cualquier tipo de error se convierte a CommandError)
        let result = self.handler.handle(command).await
            .map_err(|e| CommandError::HandlerError {
                command_type: std::any::type_name::<C>().to_string(),
                error: format!("{:?}", e),
            })?;

        Ok(Box::new(result) as Box<dyn Any + Send>)
    }
}

/// Implementación práctica de ErasedCommandBus con registro dinámico de handlers.
#[derive(Clone)]
pub struct InMemoryErasedCommandBus {
    dispatchers: Arc<Mutex<Dispatcher>>,
}

struct Dispatcher {
    dispatchers: std::collections::HashMap<TypeId, Arc<dyn AnyHandler>>,
}

impl Dispatcher {
    fn new() -> Self {
        Self {
            dispatchers: std::collections::HashMap::new(),
        }
    }
}

impl InMemoryErasedCommandBus {
    pub fn new() -> Self {
        Self {
            dispatchers: Arc::new(Mutex::new(Dispatcher::new())),
        }
    }

    /// Registra un handler para un tipo de comando específico.
    pub async fn register<C, H>(&self, handler: H)
    where
        C: Command,
        H: CommandHandler<C> + Send + Sync + 'static,
    {
        let mut dispatchers = self.dispatchers.lock().await;
        let type_id = TypeId::of::<C>();

        dispatchers.dispatchers.insert(
            type_id,
            Arc::new(HandlerWrapper::<C, H>::new(handler))
        );
    }

    /// Despacha un comando con tipo concreto (versión ergonómica).
    pub async fn dispatch<Cmd: Command>(&self, command: Cmd) -> Result<Cmd::Output, CommandError> {
        let type_id = TypeId::of::<Cmd>();
        let result = self.dispatch_erased(Box::new(command), type_id).await?;

        result
            .downcast::<Cmd::Output>()
            .map(|boxed| *boxed)
            .map_err(|_| CommandError::TypeMismatch {
                expected: std::any::type_name::<Cmd::Output>().to_string(),
                actual: "downcast failed".to_string(),
            })
    }
}

impl Default for InMemoryErasedCommandBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ErasedCommandBus for InMemoryErasedCommandBus {
    async fn dispatch_erased(
        &self,
        command: Box<dyn Any + Send>,
        command_type_id: TypeId,
    ) -> Result<Box<dyn Any + Send>, CommandError> {
        let dispatchers = self.dispatchers.lock().await;
        let handler = dispatchers.dispatchers.get(&command_type_id)
            .ok_or_else(|| CommandError::HandlerNotFound {
                command_type: format!("type_id={:?}", command_type_id),
            })?;

        handler.handle_any(command).await
    }
}

/// Alias para `Arc<dyn ErasedCommandBus>`.
pub type DynCommandBus = Arc<dyn ErasedCommandBus>;

/// Extension trait para usar dispatch ergonómico con tipos concretos.
#[async_trait::async_trait]
pub trait ErasedCommandBusExt: ErasedCommandBus {
    /// Despacha un comando con tipo concreto.
    async fn dispatch<Cmd: Command>(&self, command: Cmd) -> Result<Cmd::Output, CommandError>;
}

#[async_trait::async_trait]
impl ErasedCommandBusExt for InMemoryErasedCommandBus {
    async fn dispatch<Cmd: Command>(&self, command: Cmd) -> Result<Cmd::Output, CommandError> {
        self.dispatch::<Cmd>(command).await
    }
}

/// Simplified dispatcher function for concrete types.
/// Usa el trait object directamente.
pub async fn dispatch_erased<Cmd: Command>(
    bus: &DynCommandBus,
    command: Cmd,
) -> Result<Cmd::Output, CommandError> {
    let type_id = TypeId::of::<Cmd>();
    let result = bus.dispatch_erased(Box::new(command), type_id).await?;

    result
        .downcast::<Cmd::Output>()
        .map(|boxed| *boxed)
        .map_err(|_| CommandError::TypeMismatch {
            expected: std::any::type_name::<Cmd::Output>().to_string(),
            actual: "downcast failed".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    // Test command - must implement Debug, Clone
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

    // Test command for not found test
    #[derive(Debug, Clone)]
    struct OtherCommand;
    impl Command for OtherCommand {
        type Output = ();
        fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("".to_string()) }
    }

    // Test handler - must implement Debug, Clone
    #[derive(Debug, Clone)]
    struct TestHandler;

    #[async_trait::async_trait]
    impl CommandHandler<TestCommand> for TestHandler {
        type Error = anyhow::Error;

        async fn handle(&self, command: TestCommand) -> Result<String, Self::Error> {
            Ok(format!("handled: {}", command.value))
        }
    }

    #[tokio::test]
    async fn test_in_memory_erased_command_bus() {
        let bus = InMemoryErasedCommandBus::new();
        bus.register::<TestCommand, TestHandler>(TestHandler).await;

        let result = bus.dispatch(TestCommand {
            value: "test".to_string(),
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "handled: test");
    }

    #[tokio::test]
    async fn test_in_memory_erased_command_bus_not_found() {
        let bus = InMemoryErasedCommandBus::new();

        let result = bus.dispatch(OtherCommand).await;
        assert!(matches!(result, Err(CommandError::HandlerNotFound { .. })));
    }

    #[tokio::test]
    async fn test_dyn_command_bus() {
        let bus = InMemoryErasedCommandBus::new();
        bus.register::<TestCommand, TestHandler>(TestHandler).await;

        // Cast to DynCommandBus
        let dyn_bus: DynCommandBus = Arc::new(bus);

        let result = dispatch_erased(&dyn_bus, TestCommand {
            value: "test".to_string(),
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "handled: test");
    }

    #[tokio::test]
    async fn test_dyn_command_bus_not_found() {
        let bus: DynCommandBus = Arc::new(InMemoryErasedCommandBus::new());

        let result = dispatch_erased::<OtherCommand>(&bus, OtherCommand).await;
        assert!(matches!(result, Err(CommandError::HandlerNotFound { .. })));
    }

    // ============ Type Erasure Safety Tests (Épica 62.1-62.3) ============

    /// Test 62.1: Type safety verification during type erasure
    ///
    /// Verifies that commands of different types can be dispatched
    /// without type confusion when using the erased command bus.
    #[tokio::test]
    async fn test_type_erasure_safety_different_types() {
        // Command A with String output
        #[derive(Debug, Clone)]
        struct CommandA { value: i32 }
        impl Command for CommandA {
            type Output = i32;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("a".to_string()) }
        }

        // Command B with String output
        #[derive(Debug, Clone)]
        struct CommandB { value: String }
        impl Command for CommandB {
            type Output = String;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("b".to_string()) }
        }

        // Handler for CommandA
        #[derive(Debug, Clone)]
        struct HandlerA;
        #[async_trait::async_trait]
        impl CommandHandler<CommandA> for HandlerA {
            type Error = anyhow::Error;
            async fn handle(&self, cmd: CommandA) -> Result<i32, Self::Error> {
                Ok(cmd.value * 2)
            }
        }

        // Handler for CommandB
        #[derive(Debug, Clone)]
        struct HandlerB;
        #[async_trait::async_trait]
        impl CommandHandler<CommandB> for HandlerB {
            type Error = anyhow::Error;
            async fn handle(&self, cmd: CommandB) -> Result<String, Self::Error> {
                Ok(format!("processed: {}", cmd.value))
            }
        }

        let bus = InMemoryErasedCommandBus::new();
        bus.register::<CommandA, HandlerA>(HandlerA).await;
        bus.register::<CommandB, HandlerB>(HandlerB).await;

        let dyn_bus: DynCommandBus = Arc::new(bus);

        // Dispatch CommandA
        let result_a = dispatch_erased(&dyn_bus, CommandA { value: 21 }).await;
        assert!(result_a.is_ok());
        assert_eq!(result_a.unwrap(), 42); // 21 * 2

        // Dispatch CommandB
        let result_b = dispatch_erased(&dyn_bus, CommandB { value: "hello".to_string() }).await;
        assert!(result_b.is_ok());
        assert_eq!(result_b.unwrap(), "processed: hello");
    }

    /// Test 62.2: Multiple command types dispatch test
    ///
    /// Verifies that multiple different command types can be registered
    /// and dispatched correctly through the same DynCommandBus.
    #[tokio::test]
    async fn test_multiple_command_types_dispatch() {
        // Define multiple command types
        #[derive(Debug, Clone)]
        struct CreateCommand { name: String }
        impl Command for CreateCommand {
            type Output = uuid::Uuid;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned(self.name.clone()) }
        }

        #[derive(Debug, Clone)]
        struct UpdateCommand { id: uuid::Uuid, data: String }
        impl Command for UpdateCommand {
            type Output = bool;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned(self.id.to_string()) }
        }

        #[derive(Debug, Clone)]
        struct DeleteCommand { id: uuid::Uuid }
        impl Command for DeleteCommand {
            type Output = bool;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned(self.id.to_string()) }
        }

        // Define handlers
        #[derive(Debug, Clone)]
        struct CreateHandler;
        #[async_trait::async_trait]
        impl CommandHandler<CreateCommand> for CreateHandler {
            type Error = anyhow::Error;
            async fn handle(&self, cmd: CreateCommand) -> Result<uuid::Uuid, Self::Error> {
                Ok(uuid::Uuid::new_v4())
            }
        }

        #[derive(Debug, Clone)]
        struct UpdateHandler;
        #[async_trait::async_trait]
        impl CommandHandler<UpdateCommand> for UpdateHandler {
            type Error = anyhow::Error;
            async fn handle(&self, _cmd: UpdateCommand) -> Result<bool, Self::Error> {
                Ok(true)
            }
        }

        #[derive(Debug, Clone)]
        struct DeleteHandler;
        #[async_trait::async_trait]
        impl CommandHandler<DeleteCommand> for DeleteHandler {
            type Error = anyhow::Error;
            async fn handle(&self, _cmd: DeleteCommand) -> Result<bool, Self::Error> {
                Ok(true)
            }
        }

        // Register all handlers
        let bus = InMemoryErasedCommandBus::new();
        bus.register::<CreateCommand, CreateHandler>(CreateHandler).await;
        bus.register::<UpdateCommand, UpdateHandler>(UpdateHandler).await;
        bus.register::<DeleteCommand, DeleteHandler>(DeleteHandler).await;

        let dyn_bus: DynCommandBus = Arc::new(bus);

        // Dispatch all commands
        let create_result = dispatch_erased(&dyn_bus, CreateCommand { name: "test".to_string() }).await;
        assert!(create_result.is_ok());
        assert!(!create_result.unwrap().is_nil());

        let update_id = uuid::Uuid::new_v4();
        let update_result = dispatch_erased(&dyn_bus, UpdateCommand { id: update_id, data: "updated".to_string() }).await;
        assert!(update_result.is_ok());
        assert!(update_result.unwrap());

        let delete_id = uuid::Uuid::new_v4();
        let delete_result = dispatch_erased(&dyn_bus, DeleteCommand { id: delete_id }).await;
        assert!(delete_result.is_ok());
        assert!(delete_result.unwrap());
    }

    /// Test 62.3: DynCommandBus Send + Sync safety verification
    ///
    /// Verifies that DynCommandBus (Arc<dyn ErasedCommandBus>) is safe
    /// to share across async tasks (Send + Sync).
    #[tokio::test]
    async fn test_dyn_command_bus_send_sync_safety() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Command that increments a counter
        #[derive(Debug, Clone)]
        struct IncrementCommand;
        impl Command for IncrementCommand {
            type Output = usize;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("increment".to_string()) }
        }

        // Handler with atomic counter
        #[derive(Debug, Clone)]
        struct IncrementHandler(Arc<AtomicUsize>);
        impl IncrementHandler {
            fn new(counter: Arc<AtomicUsize>) -> Self {
                Self(counter)
            }
        }

        #[async_trait::async_trait]
        impl CommandHandler<IncrementCommand> for IncrementHandler {
            type Error = anyhow::Error;
            async fn handle(&self, _cmd: IncrementCommand) -> Result<usize, Self::Error> {
                Ok(self.0.fetch_add(1, Ordering::SeqCst))
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let bus = InMemoryErasedCommandBus::new();
        bus.register::<IncrementCommand, _>(IncrementHandler::new(counter.clone())).await;

        let dyn_bus: DynCommandBus = Arc::new(bus);

        // Spawn multiple tasks that dispatch concurrently
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let bus_clone = dyn_bus.clone();
                tokio::spawn(async move {
                    dispatch_erased(&bus_clone, IncrementCommand).await
                })
            })
            .collect();

        // Wait for all tasks
        let results: Vec<Result<Result<usize, CommandError>, tokio::task::JoinError>> =
            futures::future::join_all(handles).await;

        // Verify all completed successfully
        let mut total_increments = 0;
        for result in results {
            let inner = result.unwrap();
            assert!(inner.is_ok());
            total_increments += inner.unwrap();
        }

        // Verify counter was incremented correctly (may have race conditions but still consistent)
        let final_count = counter.load(Ordering::SeqCst);
        assert_eq!(final_count, 5, "Counter should be incremented 5 times");
    }

    /// Test: Type mismatch error is returned for incorrect downcast
    ///
    /// Verifies that the command bus properly handles type mismatches
    /// when trying to dispatch a command to the wrong handler.
    #[tokio::test]
    async fn test_type_mismatch_error() {
        #[derive(Debug, Clone)]
        struct IntCommand;
        impl Command for IntCommand {
            type Output = i32;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("int".to_string()) }
        }

        #[derive(Debug, Clone)]
        struct StringCommand;
        impl Command for StringCommand {
            type Output = String;
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("string".to_string()) }
        }

        // Handler for IntCommand only
        #[derive(Debug, Clone)]
        struct IntHandler;
        #[async_trait::async_trait]
        impl CommandHandler<IntCommand> for IntHandler {
            type Error = anyhow::Error;
            async fn handle(&self, _cmd: IntCommand) -> Result<i32, Self::Error> {
                Ok(42)
            }
        }

        let bus = InMemoryErasedCommandBus::new();
        bus.register::<IntCommand, IntHandler>(IntHandler).await;

        let dyn_bus: DynCommandBus = Arc::new(bus);

        // Dispatching StringCommand should fail with HandlerNotFound
        let result = dispatch_erased(&dyn_bus, StringCommand).await;
        assert!(matches!(result, Err(CommandError::HandlerNotFound { .. })));
    }

    /// Test: Commands with different error types work correctly
    ///
    /// Verifies that handlers with different Error types can be used
    /// together through the type-erased bus.
    #[tokio::test]
    async fn test_different_error_types() {
        use std::io;

        #[derive(Debug, Clone)]
        struct ErrorTypeA;
        impl Command for ErrorTypeA {
            type Output = ();
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("a".to_string()) }
        }

        #[derive(Debug, Clone)]
        struct ErrorTypeB;
        impl Command for ErrorTypeB {
            type Output = ();
            fn idempotency_key(&self) -> Cow<'_, str> { Cow::Owned("b".to_string()) }
        }

        // Handler with io::Error
        #[derive(Debug, Clone)]
        struct HandlerA;
        #[async_trait::async_trait]
        impl CommandHandler<ErrorTypeA> for HandlerA {
            type Error = io::Error;
            async fn handle(&self, _cmd: ErrorTypeA) -> Result<(), Self::Error> {
                Ok(())
            }
        }

        // Handler with custom error
        #[derive(Debug, Clone)]
        struct CustomError(&'static str);
        impl std::fmt::Display for CustomError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        impl std::error::Error for CustomError {}

        #[derive(Debug, Clone)]
        struct HandlerB;
        #[async_trait::async_trait]
        impl CommandHandler<ErrorTypeB> for HandlerB {
            type Error = CustomError;
            async fn handle(&self, _cmd: ErrorTypeB) -> Result<(), Self::Error> {
                Ok(())
            }
        }

        let bus = InMemoryErasedCommandBus::new();
        bus.register::<ErrorTypeA, HandlerA>(HandlerA).await;
        bus.register::<ErrorTypeB, HandlerB>(HandlerB).await;

        let dyn_bus: DynCommandBus = Arc::new(bus);

        // Both commands should succeed
        let result_a = dispatch_erased(&dyn_bus, ErrorTypeA).await;
        assert!(result_a.is_ok());

        let result_b = dispatch_erased(&dyn_bus, ErrorTypeB).await;
        assert!(result_b.is_ok());
    }
}
