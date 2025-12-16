use async_trait::async_trait;
use futures::stream::BoxStream;
use hodei_jobs_domain::event_bus::{EventBus, EventBusError};
use hodei_jobs_domain::events::DomainEvent;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tracing::{debug, error, info, instrument};

/// Implementación del EventBus usando PostgreSQL NOTIFY/LISTEN
pub struct PostgresEventBus {
    pool: PgPool,
}

impl PostgresEventBus {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventBus for PostgresEventBus {
    #[instrument(skip(self, event), fields(event_type = ?event))]
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusError> {
        let payload = serde_json::to_string(event)
            .map_err(|e| EventBusError::SerializationError(e.to_string()))?;

        // Determinar el canal basado en el tipo de evento o usar un canal global 'events'
        // Por simplicidad en esta fase, usaremos 'hodei_events' para todo
        let channel = "hodei_events";

        debug!("Publishing event to channel {}: {}", channel, payload);

        // Usamos pg_notify directamente
        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(channel)
            .bind(payload)
            .execute(&self.pool)
            .await
            .map_err(|e| EventBusError::PublishError(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<BoxStream<'static, Result<DomainEvent, EventBusError>>, EventBusError> {
        let pool = self.pool.clone();
        let channel = topic.to_string(); // 'hodei_events' normalmente

        // Crear listener
        let mut listener = PgListener::connect_with(&pool)
            .await
            .map_err(|e| EventBusError::ConnectionError(e.to_string()))?;

        listener
            .listen(&channel)
            .await
            .map_err(|e| EventBusError::SubscribeError(e.to_string()))?;

        info!("Subscribed to channel: {}", channel);

        // Convertir listener en stream
        let stream = async_stream::stream! {
            loop {
                match listener.recv().await {
                    Ok(notification) => {
                        let payload = notification.payload();
                        match serde_json::from_str::<DomainEvent>(payload) {
                            Ok(event) => yield Ok(event),
                            Err(e) => {
                                error!("Failed to deserialize event: {}", e);
                                yield Err(EventBusError::SerializationError(e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving notification: {}", e);
                        // Intentar reconexión o terminar stream?
                        // Por ahora terminamos con error, el consumidor debería re-suscribir
                        yield Err(EventBusError::ConnectionError(e.to_string()));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}
