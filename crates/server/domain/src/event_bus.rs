use crate::events::DomainEvent;
use async_trait::async_trait;
use futures::stream::BoxStream;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EventBusError {
    #[error("Failed to publish event: {0}")]
    PublishError(String),
    #[error("Failed to subscribe to channel: {0}")]
    SubscribeError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

/// Define la interfaz para el bus de eventos del dominio.
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publica un evento de dominio
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusError>;

    /// Se suscribe a un topic/canal y devuelve un stream de eventos
    async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError>;
}

impl From<EventBusError> for crate::shared_kernel::DomainError {
    fn from(err: EventBusError) -> Self {
        crate::shared_kernel::DomainError::InfrastructureError {
            message: err.to_string(),
        }
    }
}
