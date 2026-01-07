// Handler Registry - TypeId-based dynamic handler storage

use super::*;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Storage for handlers keyed by command TypeId.
#[derive(Debug, Default)]
pub struct HandlerRegistry {
    handlers: HashMap<TypeId, HandlerBox>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register<C: Command + 'static, H: CommandHandler<C> + 'static>(
        &mut self,
        handler: Arc<H>,
    ) {
        let type_id = TypeId::of::<C>();
        let boxed: Box<dyn Any + Send + Sync> = Box::new(handler);
        self.handlers.insert(type_id, boxed);
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

/// In-memory implementation of handler storage.
#[derive(Debug, Default)]
pub struct InMemoryHandlerStorage {
    registry: Arc<parking_lot::Mutex<HandlerRegistry>>,
}

impl InMemoryHandlerStorage {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(parking_lot::Mutex::new(HandlerRegistry::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handler_registry_empty() {
        let registry = HandlerRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn test_in_memory_handler_storage() {
        let storage = InMemoryHandlerStorage::new();
        let registry = storage.registry.lock();
        assert!(registry.is_empty());
    }
}
