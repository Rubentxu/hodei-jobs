use async_trait::async_trait;
use futures::stream::BoxStream;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tracing::{error, info, instrument};

/// Implementación del EventBus usando PostgreSQL NOTIFY/LISTEN
#[derive(Clone)]
pub struct PostgresEventBus {
    pool: PgPool,
}

impl PostgresEventBus {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        info!("Applying domain_events migration...");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS domain_events (
                id UUID PRIMARY KEY,
                occurred_at TIMESTAMPTZ NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                aggregate_id VARCHAR(255) NOT NULL,
                correlation_id VARCHAR(255),
                actor VARCHAR(255),
                payload JSONB NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_domain_events_occurred_at ON domain_events(occurred_at)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_domain_events_aggregate_id ON domain_events(aggregate_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_domain_events_correlation_id ON domain_events(correlation_id)")
            .execute(&self.pool)
            .await?;

        info!("✅ Domain Events table migration applied");
        Ok(())
    }
}

#[async_trait]
impl EventBus for PostgresEventBus {
    #[instrument(skip(self, event), fields(event_type = ?event))]
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventBusError> {
        // Determinar el canal basado en el tipo de evento o usar un canal global 'events'
        // Por simplicidad en esta fase, usaremos 'hodei_events' para todo
        let channel = "hodei_events";

        // Extraer metadatos del evento
        let event_id = uuid::Uuid::new_v4();
        let occurred_at = event.occurred_at();
        let event_type = event.event_type();
        let aggregate_id = event.aggregate_id();
        let correlation_id = event.correlation_id();
        let actor = event.actor();

        // OPTIMIZACIÓN: Single-pass DomainEvent → serde_json::Value
        // En lugar de to_string() + from_str() (doble serialización),
        // usamos to_value() directamente
        let payload_json = serde_json::to_value(event)
            .map_err(|e| EventBusError::SerializationError(e.to_string()))?;

        // Para pg_notify necesitamos String, reutilizamos el Value
        let payload_string = payload_json.to_string();

        info!(
            "🔥 Publishing event to channel {}: {}",
            channel, payload_string
        );

        // Persistir el evento para audit log (Timeline)
        sqlx::query(
            r#"
            INSERT INTO domain_events
            (id, occurred_at, event_type, aggregate_id, correlation_id, actor, payload)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(event_id)
        .bind(occurred_at)
        .bind(event_type)
        .bind(aggregate_id)
        .bind(correlation_id)
        .bind(actor)
        .bind(&payload_json) // sqlx serializa Value a JSONB
        .execute(&self.pool)
        .await
        .map_err(|e| EventBusError::PublishError(format!("Failed to persist event: {}", e)))?;

        info!("💾 Event persisted with ID: {}", event_id);

        // CRITICAL: pg_notify requires a DEDICATED connection (not from pool)
        let mut conn = self.pool.acquire().await.map_err(|e| {
            EventBusError::PublishError(format!("Failed to acquire connection: {}", e))
        })?;

        info!("✅ Acquired dedicated connection for NOTIFY");

        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(channel)
            .bind(payload_string)
            .execute(&mut *conn)
            .await
            .map_err(|e| EventBusError::PublishError(e.to_string()))?;

        info!("✅ NOTIFY executed successfully");

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
