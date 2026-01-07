// Handler module - HandlerBox type for dynamic handler storage

use super::*;
use std::any::Any;
use std::marker::PhantomData;
use std::sync::Arc;

/// Boxed handler type for dynamic storage.
pub type HandlerBox = Box<dyn Any + Send + Sync>;

/// Wrapper for command handlers that erases the concrete type.
#[derive(Debug, Clone)]
pub struct BoxedHandler<C: Command> {
    inner: Arc<HandlerBox>,
    _phantom: PhantomData<C>,
}

impl<C: Command> BoxedHandler<C> {
    pub fn new<H: CommandHandler<C>>(handler: H) -> Self
    where
        H: 'static,
    {
        let boxed: Box<dyn Any + Send + Sync> = Box::new(Arc::new(handler));
        Self {
            inner: Arc::new(boxed),
            _phantom: PhantomData,
        }
    }

    pub fn downcast_ref<H: CommandHandler<C>>(&self) -> Option<&H> {
        self.inner.downcast_ref::<Arc<H>>().map(|arc| arc.as_ref())
    }
}
