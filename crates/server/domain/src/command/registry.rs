// Handler Registry - TypeId-based dynamic handler storage

use super::*;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Storage for handlers keyed by command TypeId.
#[derive(Debug, Default)]
pub struct HandlerRegistry {
    handlers: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register<C: Command + 'static, H: CommandHandler<C> + Send + Sync + 'static>(
        &mut self,
        handler: H,
    ) {
        self.insert::<C, H>(handler);
    }

    /// Insert a handler for a command type (internal use)
    pub(crate) fn insert<C: Command + 'static, H: CommandHandler<C> + Send + Sync + 'static>(
        &mut self,
        handler: H,
    ) {
        let type_id = TypeId::of::<C>();
        let boxed: Box<dyn Any + Send + Sync> = Box::new(Arc::new(handler));
        self.handlers.insert(type_id, boxed);
    }

    /// Get a handler for a specific command type.
    pub fn get_handler<C: Command + 'static>(&self) -> Option<&Box<dyn Any + Send + Sync>> {
        let type_id = TypeId::of::<C>();
        self.handlers.get(&type_id)
    }

    /// Check if a handler is registered for a command type.
    pub fn has_handler<C: Command + 'static>(&self) -> bool {
        let type_id = TypeId::of::<C>();
        self.handlers.contains_key(&type_id)
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
    pub(crate) registry: Arc<parking_lot::Mutex<HandlerRegistry>>,
}

impl InMemoryHandlerStorage {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(parking_lot::Mutex::new(HandlerRegistry::new())),
        }
    }

    /// Returns a reference to the underlying registry
    pub fn registry(&self) -> &Arc<parking_lot::Mutex<HandlerRegistry>> {
        &self.registry
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
        let registry = storage.registry().lock();
        assert!(registry.is_empty());
    }
}
