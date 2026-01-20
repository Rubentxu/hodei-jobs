//! PostgreSQL HistoryReplayer implementation for saga-engine.
//!
//! This module provides a production-ready PostgreSQL implementation of the
//! [`HistoryReplayer`] trait for reconstructing saga state from event history.

use async_trait::async_trait;
use saga_engine_core::event::{HistoryEvent, SagaId};
use saga_engine_core::port::EventStore;
use saga_engine_core::port::replay::{
    Applicator, HistoryReplayer, ReplayConfig, ReplayError, ReplayResult,
};
use sqlx::postgres::PgPool;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::{debug, error, instrument};

/// PostgreSQL-backed HistoryReplayer.
pub struct PostgresReplayer<E = sqlx::Error>
where
    E: Debug + Send + Sync + 'static,
{
    /// Database pool for PostgreSQL connections.
    pool: Arc<PgPool>,
    /// Event store for retrieving event history.
    event_store: Arc<dyn EventStore<Error = E> + Send + Sync>,
}

impl<E> std::fmt::Debug for PostgresReplayer<E>
where
    E: Debug + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresReplayer")
            .field("pool", &self.pool)
            .finish_non_exhaustive()
    }
}

impl<E> PostgresReplayer<E>
where
    E: Debug + Send + Sync + 'static,
{
    /// Create a new PostgresReplayer.
    pub fn new(
        pool: Arc<PgPool>,
        event_store: Arc<dyn EventStore<Error = E> + Send + Sync>,
    ) -> Self {
        Self { pool, event_store }
    }
}

#[async_trait]
impl<E, T> HistoryReplayer<T> for PostgresReplayer<E>
where
    E: Debug + Send + Sync + 'static,
    T: Default + Clone + Send + Sync + 'static + Applicator,
{
    type Error = E;

    #[instrument(skip(self, state, events, config))]
    async fn replay(
        &self,
        mut state: T,
        events: &[HistoryEvent],
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<T>, ReplayError<Self::Error>> {
        let start = std::time::Instant::now();
        let config = config.unwrap_or_default();

        let mut last_event_id = 0u64;
        let mut events_replayed = 0;

        for event in events {
            // Validate sequence
            if event.event_id.0 <= last_event_id && events_replayed > 0 {
                return Err(ReplayError::invalid_sequence(
                    last_event_id + 1,
                    event.event_id.0,
                ));
            }
            last_event_id = event.event_id.0;

            // Apply event to state
            state.apply(event).map_err(|e| {
                error!("Failed to apply event {}: {}", event.event_id, e);
                ReplayError::Apply(e)
            })?;

            events_replayed += 1;

            // Check limits
            if config.max_events > 0 && events_replayed >= config.max_events {
                break;
            }
            if start.elapsed() > config.timeout {
                return Err(ReplayError::Timeout);
            }
        }

        Ok(ReplayResult {
            state,
            events_replayed,
            replay_duration: start.elapsed(),
            last_event_id,
        })
    }

    #[instrument(skip(self, config))]
    async fn replay_from_event_id(
        &self,
        saga_id: &SagaId,
        from_event_id: u64,
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<T>, ReplayError<Self::Error>> {
        let events = self
            .event_store
            .get_history_from(saga_id, from_event_id)
            .await
            .map_err(ReplayError::Storage)?;

        self.replay(T::default(), &events, config).await
    }

    #[instrument(skip(self, config))]
    async fn get_current_state(
        &self,
        saga_id: &SagaId,
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<T>, ReplayError<Self::Error>> {
        let config = config.unwrap_or_default();

        let mut state = T::default();
        let mut from_event_id = 0u64;

        // Try to load latest snapshot
        if config.use_snapshots {
            if let Some((snapshot_id, snapshot_data)) = self
                .event_store
                .get_latest_snapshot(saga_id)
                .await
                .map_err(ReplayError::Storage)?
            {
                debug!("Found snapshot at event {}", snapshot_id);
                state = T::from_snapshot(&snapshot_data).map_err(|e| {
                    error!("Failed to deserialize snapshot: {}", e);
                    ReplayError::Deserialization(e)
                })?;
                from_event_id = snapshot_id;
            }
        }

        // Fetch remaining events
        let events = self
            .event_store
            .get_history_from(saga_id, from_event_id)
            .await
            .map_err(ReplayError::Storage)?;

        self.replay(state, &events, Some(config)).await
    }

    async fn validate_sequence(
        &self,
        events: &[HistoryEvent],
        allow_gaps: bool,
    ) -> Result<usize, ReplayError<Self::Error>> {
        let mut last_id = 0u64;
        let mut invalid = 0;
        for (i, event) in events.iter().enumerate() {
            if i > 0 {
                if !allow_gaps && event.event_id.0 != last_id + 1 {
                    invalid += 1;
                }
                if event.event_id.0 <= last_id {
                    invalid += 1;
                }
            }
            last_id = event.event_id.0;
        }
        Ok(invalid)
    }
}
