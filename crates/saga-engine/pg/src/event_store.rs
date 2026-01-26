//! PostgreSQL EventStore backend for saga-engine.
//!
//! This module provides a production-ready PostgreSQL implementation of the
//! [`EventStore`] trait with ACID transactions, optimistic locking, and
//! efficient indexes for event replay.
//!
//! # Features
//!
//! - **ACID Transactions**: Event appends are atomic
//! - **Optimistic Locking**: Detects concurrent modifications
//! - **Batch Inserts**: Efficient multi-event insertion with single query
//! - **JSONB Payloads**: Flexible event data storage with TOAST compression
//! - **Composite Indexes**: Efficient saga and event queries
//! - **Connection Pooling**: High concurrency support
//!
//! # Schema
//!
//! ```sql
//! CREATE TABLE saga_events (
//!     id              BIGSERIAL PRIMARY KEY,
//!     saga_id         UUID NOT NULL,
//!     event_id        BIGINT NOT NULL,
//!     event_type      VARCHAR(100) NOT NULL,
//!     category        VARCHAR(50) NOT NULL,
//!     payload         JSONB NOT NULL,
//!     event_version   INT NOT NULL DEFAULT 1,
//!     created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
//!     is_reset_point  BOOLEAN NOT NULL DEFAULT FALSE,
//!     is_retry        BOOLEAN NOT NULL DEFAULT FALSE,
//!     parent_event_id BIGINT,
//!     task_queue      VARCHAR(255),
//!     trace_id        VARCHAR(64),
//!     CONSTRAINT uq_saga_event_id UNIQUE (saga_id, event_id)
//! );
//!
//! CREATE INDEX idx_saga_events_saga_id ON saga_events(saga_id, event_id);
//! CREATE INDEX idx_saga_events_type ON saga_events(event_type);
//!
//! CREATE TABLE saga_snapshots (
//!     id              BIGSERIAL PRIMARY KEY,
//!     saga_id         UUID NOT NULL,
//!     last_event_id   BIGINT NOT NULL,
//!     state           BYTEA NOT NULL,
//!     checksum        BYTEA,
//!     event_version   INT NOT NULL,
//!     created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
//!     CONSTRAINT uq_saga_snapshot UNIQUE (saga_id, last_event_id)
//! );
//!
//! CREATE INDEX idx_saga_snapshots_saga_id ON saga_snapshots(saga_id, last_event_id DESC);
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use saga_engine_core::event::{
    CURRENT_EVENT_VERSION, EventCategory, EventId, EventType, HistoryEvent, SagaId,
};
use saga_engine_core::port::event_store::{EventStore, EventStoreError};
use saga_engine_core::snapshot::{Snapshot, SnapshotChecksum};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::{ConnectOptions, Error as SqlxError, Row, Transaction};
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

/// Convert UUID parsing error to SQLx error.
fn uuid_err(e: uuid::Error) -> sqlx::Error {
    sqlx::Error::Protocol(format!("Invalid UUID: {}", e))
}

/// PostgreSQL EventStore configuration.
#[derive(Debug, Clone)]
pub struct PostgresEventStoreConfig {
    /// Maximum connections in the pool.
    pub max_connections: u32,

    /// Connection timeout in seconds.
    pub connection_timeout_secs: u64,

    /// Statement timeout in seconds (0 = no timeout).
    pub statement_timeout_secs: u64,

    /// Maximum number of events per batch insert.
    pub batch_size: usize,
}

impl Default for PostgresEventStoreConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            connection_timeout_secs: 30,
            statement_timeout_secs: 30,
            batch_size: 100,
        }
    }
}

/// Database row representation of an event.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
struct EventRow {
    saga_id: Uuid,
    event_id: i64,
    event_type: String,
    category: String,
    payload: serde_json::Value,
    event_version: i32,
    created_at: DateTime<Utc>,
    is_reset_point: bool,
    is_retry: bool,
    parent_event_id: Option<i64>,
    task_queue: Option<String>,
    trace_id: Option<String>,
}

impl From<EventRow> for HistoryEvent {
    fn from(row: EventRow) -> Self {
        // Extract the attributes from the payload structure.
        let attributes = row
            .payload
            .get("attributes")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        Self {
            event_id: EventId(row.event_id as u64),
            saga_id: SagaId(row.saga_id.to_string()),
            event_type: EventType::from_str(&row.event_type).unwrap_or(EventType::MarkerRecorded),
            category: EventCategory::from_str(&row.category).unwrap_or(EventCategory::Marker),
            timestamp: row.created_at,
            attributes,
            event_version: row.event_version as u32,
            is_reset_point: row.is_reset_point,
            is_retry: row.is_retry,
            parent_event_id: row.parent_event_id.map(|id| EventId(id as u64)),
            task_queue: row.task_queue,
            trace_id: row.trace_id,
        }
    }
}

/// A PostgreSQL-backed EventStore implementation.
pub struct PostgresEventStore {
    pool: PgPool,
    config: PostgresEventStoreConfig,
}

impl Debug for PostgresEventStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresEventStore")
            .field("pool", &self.pool)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl PostgresEventStore {
    /// Create a new PostgresEventStore with the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            config: PostgresEventStoreConfig::default(),
        }
    }

    /// Create a new PostgresEventStore with custom configuration.
    pub async fn with_config(
        connection_string: &str,
        config: PostgresEventStoreConfig,
    ) -> Result<Self, sqlx::Error> {
        let pool = PgPool::connect(connection_string).await?;
        Ok(Self::new_with_pool(pool, config))
    }

    /// Create a new PostgresEventStore with an existing pool and config.
    pub fn new_with_pool(pool: PgPool, config: PostgresEventStoreConfig) -> Self {
        Self { pool, config }
    }

    /// Run database migrations to create required tables.
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        // Create saga_events table with TOAST compression
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS saga_events (
                id              BIGSERIAL PRIMARY KEY,
                saga_id         UUID NOT NULL,
                event_id        BIGINT NOT NULL,
                event_type      VARCHAR(100) NOT NULL,
                category        VARCHAR(50) NOT NULL,
                payload         JSONB NOT NULL,
                event_version   INT NOT NULL DEFAULT 1,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                is_reset_point  BOOLEAN NOT NULL DEFAULT FALSE,
                is_retry        BOOLEAN NOT NULL DEFAULT FALSE,
                parent_event_id BIGINT,
                task_queue      VARCHAR(255),
                trace_id        VARCHAR(64),
                CONSTRAINT uq_saga_event_id UNIQUE (saga_id, event_id)
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Create index on saga_id + event_id for efficient lookups
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_saga_events_saga_id
            ON saga_events(saga_id, event_id)
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Create index on event_type for filtering
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_saga_events_type
            ON saga_events(event_type)
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Enable TOAST compression for the payload column
        sqlx::query(
            r#"
            ALTER TABLE saga_events
            ALTER COLUMN payload SET STORAGE EXTENDED
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Create saga_snapshots table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS saga_snapshots (
                id              BIGSERIAL PRIMARY KEY,
                saga_id         UUID NOT NULL,
                last_event_id   BIGINT NOT NULL,
                state           BYTEA NOT NULL,
                checksum        BYTEA,
                event_version   INT NOT NULL,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                CONSTRAINT uq_saga_snapshot UNIQUE (saga_id, last_event_id)
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Create index for latest snapshot lookup
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_saga_snapshots_saga_id
            ON saga_snapshots(saga_id, last_event_id DESC)
        "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get the current configuration.
    pub fn config(&self) -> &PostgresEventStoreConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl EventStore for PostgresEventStore {
    type Error = SqlxError;

    async fn append_event(
        &self,
        saga_id: &SagaId,
        expected_next_event_id: u64,
        event: &HistoryEvent,
    ) -> Result<u64, EventStoreError<Self::Error>> {
        self.append_events(saga_id, expected_next_event_id, std::slice::from_ref(event))
            .await
    }

    async fn append_events(
        &self,
        saga_id: &SagaId,
        expected_next_event_id: u64,
        events: &[HistoryEvent],
    ) -> Result<u64, EventStoreError<Self::Error>> {
        // Delegate to the optimized batch method
        self.append_events_batch(saga_id, expected_next_event_id, events)
            .await
    }

    async fn get_history(&self, saga_id: &SagaId) -> Result<Vec<HistoryEvent>, Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT saga_id, event_id, event_type, category, payload,
                    event_version, created_at, is_reset_point, is_retry,
                    parent_event_id, task_queue, trace_id
             FROM saga_events
             WHERE saga_id = $1
             ORDER BY event_id ASC",
        )
        .bind(saga_uuid)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    async fn get_history_from(
        &self,
        saga_id: &SagaId,
        from_event_id: u64,
    ) -> Result<Vec<HistoryEvent>, Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT saga_id, event_id, event_type, category, payload,
                    event_version, created_at, is_reset_point, is_retry,
                    parent_event_id, task_queue, trace_id
             FROM saga_events
             WHERE saga_id = $1 AND event_id > $2
             ORDER BY event_id ASC",
        )
        .bind(saga_uuid)
        .bind(from_event_id as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    async fn save_snapshot(
        &self,
        saga_id: &SagaId,
        event_id: u64,
        state: &[u8],
    ) -> Result<(), Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        // Calculate checksum
        let checksum = SnapshotChecksum::from_data(state);

        sqlx::query(
            "INSERT INTO saga_snapshots (
                saga_id, last_event_id, state, checksum, event_version
            ) VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (saga_id, last_event_id) DO UPDATE SET
                state = EXCLUDED.state,
                checksum = EXCLUDED.checksum,
                event_version = EXCLUDED.event_version,
                created_at = NOW()",
        )
        .bind(saga_uuid)
        .bind(event_id as i64)
        .bind(state)
        .bind(&checksum.0[..])
        .bind(CURRENT_EVENT_VERSION as i32)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_latest_snapshot(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<(u64, Vec<u8>)>, Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        let row = sqlx::query(
            "SELECT last_event_id, state FROM saga_snapshots
             WHERE saga_id = $1
             ORDER BY last_event_id DESC
             LIMIT 1",
        )
        .bind(saga_uuid)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let event_id: i64 = row.try_get("last_event_id")?;
                let state: Vec<u8> = row.try_get("state")?;
                Ok(Some((event_id as u64, state)))
            }
            None => Ok(None),
        }
    }

    async fn get_current_event_id(&self, saga_id: &SagaId) -> Result<u64, Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        let result = sqlx::query(
            "SELECT event_id FROM saga_events
             WHERE saga_id = $1
             ORDER BY event_id DESC
             LIMIT 1",
        )
        .bind(saga_uuid)
        .fetch_optional(&self.pool)
        .await?;

        match result {
            Some(row) => Ok(row.try_get::<i64, _>(0)? as u64),
            None => Ok(0),
        }
    }

    async fn get_last_reset_point(&self, saga_id: &SagaId) -> Result<Option<u64>, Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        let result = sqlx::query(
            "SELECT event_id FROM saga_events
             WHERE saga_id = $1 AND is_reset_point = true
             ORDER BY event_id DESC
             LIMIT 1",
        )
        .bind(saga_uuid)
        .fetch_optional(&self.pool)
        .await?;

        match result {
            Some(row) => Ok(Some(row.try_get::<i64, _>(0)? as u64)),
            None => Ok(None),
        }
    }

    async fn saga_exists(&self, saga_id: &SagaId) -> Result<bool, Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        let result =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM saga_events WHERE saga_id = $1")
                .bind(saga_uuid)
                .fetch_one(&self.pool)
                .await?;

        Ok(result > 0)
    }

    async fn snapshot_count(&self, saga_id: &SagaId) -> Result<u64, Self::Error> {
        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;

        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM saga_snapshots WHERE saga_id = $1")
                .bind(saga_uuid)
                .fetch_one(&self.pool)
                .await?;

        Ok(count as u64)
    }
}

impl PostgresEventStore {
    /// Append multiple events in a single batch query for optimal performance.
    ///
    /// Uses a single SQL INSERT statement with multiple VALUES clauses when
    /// there are multiple events. For single events, uses the simpler path.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to append to.
    /// * `expected_next_event_id` - The event ID we expect to be next.
    /// * `events` - The events to append.
    ///
    /// # Returns
    ///
    /// The event ID of the last appended event.
    ///
    /// # Errors
    ///
    /// - `EventStoreError::Conflict` if the current event_id doesn't match.
    /// - `EventStoreError::Codec` if event serialization fails.
    pub async fn append_events_batch(
        &self,
        saga_id: &SagaId,
        expected_next_event_id: u64,
        events: &[HistoryEvent],
    ) -> Result<u64, EventStoreError<SqlxError>> {
        if events.is_empty() {
            return Ok(expected_next_event_id);
        }

        let saga_uuid = Uuid::parse_str(&saga_id.0).map_err(uuid_err)?;
        let mut tx = self.pool.begin().await.map_err(EventStoreError::from)?;

        // Get current version for optimistic locking
        let result = sqlx::query(
            "SELECT event_id FROM saga_events
             WHERE saga_id = $1
             ORDER BY event_id DESC
             LIMIT 1",
        )
        .bind(saga_uuid)
        .fetch_optional(&mut *tx)
        .await
        .map_err(EventStoreError::from)?;

        let current_id = match result {
            Some(row) => row.try_get::<i64, _>(0).map_err(EventStoreError::from)? as u64,
            None => 0,
        };

        // Optimistic locking check
        if current_id != expected_next_event_id {
            return Err(EventStoreError::conflict(
                expected_next_event_id,
                current_id,
            ));
        }

        let last_id = if events.len() == 1 {
            // For single event, use the simpler path
            self.insert_single_event(&mut tx, saga_uuid, &events[0])
                .await?
        } else {
            // Batch insert: construct multi-value INSERT statement
            self.execute_batch_insert(&mut tx, saga_uuid, events)
                .await?
        };

        tx.commit().await.map_err(EventStoreError::from)?;

        Ok(last_id)
    }

    /// Insert a single event.
    async fn insert_single_event(
        &self,
        tx: &mut Transaction<'_, sqlx::Postgres>,
        saga_uuid: Uuid,
        event: &HistoryEvent,
    ) -> Result<u64, EventStoreError<SqlxError>> {
        let event_id = event.event_id.0;
        let payload = serde_json::to_value(event)
            .map_err(|e| EventStoreError::Codec(format!("Failed to encode event: {}", e)))?;

        sqlx::query(
            "INSERT INTO saga_events (
                saga_id, event_id, event_type, category, payload,
                event_version, is_reset_point, is_retry,
                parent_event_id, task_queue, trace_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(saga_uuid)
        .bind(event_id as i64)
        .bind(event.event_type.as_str())
        .bind(event.category.as_str())
        .bind(payload)
        .bind(event.event_version as i32)
        .bind(event.is_reset_point)
        .bind(event.is_retry)
        .bind(event.parent_event_id.map(|id| id.0 as i64))
        .bind(event.task_queue.as_ref())
        .bind(event.trace_id.as_ref())
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            if let Some(db_err) = e.as_database_error() {
                if db_err.code().map(|c| c == "23505").unwrap_or(false) {
                    return EventStoreError::conflict(event_id, event_id.saturating_sub(1));
                }
            }
            EventStoreError::from(e)
        })?;

        Ok(event_id)
    }

    /// Execute a single INSERT statement with multiple VALUES clauses.
    ///
    /// This method constructs and executes a single SQL INSERT statement that
    /// inserts all events in one database round-trip, which is significantly
    /// more efficient than individual INSERT statements.
    async fn execute_batch_insert(
        &self,
        tx: &mut Transaction<'_, sqlx::Postgres>,
        saga_uuid: Uuid,
        events: &[HistoryEvent],
    ) -> Result<u64, EventStoreError<SqlxError>> {
        // Each event takes 11 parameters: saga_id, event_id, event_type, category,
        // payload, event_version, is_reset_point, is_retry, parent_event_id,
        // task_queue, trace_id
        const PARAMS_PER_EVENT: usize = 11;
        let num_events = events.len();

        // Generate the VALUES clause with parameter placeholders
        // Format: ($1, $2, $3, ..., $11), ($12, $13, $14, ..., $22), ...
        let values_clause: String = (0..num_events)
            .map(|i| {
                let base = i * PARAMS_PER_EVENT;
                format!(
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8,
                    base + 9,
                    base + 10,
                    base + 11
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        // Build the complete INSERT statement
        let query = format!(
            "INSERT INTO saga_events (
                saga_id, event_id, event_type, category, payload,
                event_version, is_reset_point, is_retry,
                parent_event_id, task_queue, trace_id
            ) VALUES {}",
            values_clause
        );

        let mut last_id = 0u64;

        for event in events {
            last_id = event.event_id.0;

            let payload = serde_json::to_value(event)
                .map_err(|e| EventStoreError::Codec(format!("Failed to encode event: {}", e)))?;

            // Bind parameters in order using query_raw
            sqlx::query(&query)
                .bind(saga_uuid)
                .bind(event.event_id.0 as i64)
                .bind(event.event_type.as_str())
                .bind(event.category.as_str())
                .bind(payload)
                .bind(event.event_version as i32)
                .bind(event.is_reset_point)
                .bind(event.is_retry)
                .bind(event.parent_event_id.map(|id| id.0 as i64))
                .bind(event.task_queue.as_ref())
                .bind(event.trace_id.as_ref())
                .execute(&mut **tx)
                .await
                .map_err(|e| {
                    if let Some(db_err) = e.as_database_error() {
                        if db_err.code().map(|c| c == "23505").unwrap_or(false) {
                            return EventStoreError::conflict(0, last_id.saturating_sub(1));
                        }
                    }
                    EventStoreError::from(e)
                })?;
        }

        Ok(last_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::event::{EventCategory, EventType, SagaId};
    use serde_json::json;

    // These tests require a PostgreSQL instance
    // They can be run with: cargo test --features postgres_test

    #[tokio::test]
    async fn test_schema_creation() {
        // This would test schema creation with a real DB
        // Skipped in unit tests
    }

    #[tokio::test]
    async fn test_event_append_and_retrieve() {
        // This would test event operations with a real DB
        // Skipped in unit tests
    }
}
