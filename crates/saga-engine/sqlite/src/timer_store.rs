//! # SQLite TimerStore Implementation
//!
//! This module provides [`SqliteTimerStore`] for durable timer storage.
//!
//! Timers are stored in SQLite and can be queried by scheduled time.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use saga_engine_core::event::SagaId;
use saga_engine_core::port::timer_store::{
    DurableTimer, TimerStore, TimerStoreError, TimerStatus, TimerType,
};
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

/// Configuration for [`SqliteTimerStore`].
#[derive(Debug, Clone)]
pub struct SqliteTimerStoreConfig {
    /// Cleanup interval in seconds.
    pub cleanup_interval_secs: u64,
    /// Number of timers to return per fetch.
    pub fetch_batch_size: u64,
}

impl Default for SqliteTimerStoreConfig {
    fn default() -> Self {
        Self {
            cleanup_interval_secs: 60,
            fetch_batch_size: 100,
        }
    }
}

/// SQLite TimerStore implementation.
///
/// This implementation provides durable timer storage with:
/// - SQLite-backed persistence
/// - Efficient lookup by fire time
/// - Automatic cleanup of expired timers
///
/// # Examples
///
/// ```ignore
/// use saga_engine_sqlite::SqliteTimerStore;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let timer_store = SqliteTimerStore::new("/tmp/my-app.db").await?;
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SqliteTimerStore {
    pool: Arc<SqlitePool>,
}

impl SqliteTimerStore {
    /// Create a new SQLite TimerStore with file-based persistence.
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, SqliteTimerStoreError> {
        let pool = SqlitePool::connect(&format!("sqlite:{}", path.as_ref().display()))
            .await
            .map_err(SqliteTimerStoreError::Backend)?;

        Self::init_pool(pool, &SqliteTimerStoreConfig::default()).await
    }

    /// Create a new in-memory SQLite TimerStore.
    pub async fn in_memory() -> Result<Self, SqliteTimerStoreError> {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .map_err(SqliteTimerStoreError::Backend)?;

        Self::init_pool(pool, &SqliteTimerStoreConfig::default()).await
    }

    async fn init_pool(
        pool: SqlitePool,
        _config: &SqliteTimerStoreConfig,
    ) -> Result<Self, SqliteTimerStoreError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS saga_timers (
                timer_id TEXT PRIMARY KEY,
                saga_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                fire_at INTEGER NOT NULL,
                timer_type TEXT NOT NULL,
                status TEXT NOT NULL,
                attributes BLOB,
                attempt INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(SqliteTimerStoreError::Backend)?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_saga_timers_fire_at ON saga_timers(fire_at)
            "#,
        )
        .execute(&pool)
        .await
        .map_err(SqliteTimerStoreError::Backend)?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_saga_timers_saga_id ON saga_timers(saga_id)
            "#,
        )
        .execute(&pool)
        .await
        .map_err(SqliteTimerStoreError::Backend)?;

        Ok(Self {
            pool: Arc::new(pool),
        })
    }
}

#[async_trait]
impl TimerStore for SqliteTimerStore {
    type Error = SqliteTimerStoreError;

    async fn create_timer(&self, timer: &DurableTimer) -> Result<(), TimerStoreError<Self::Error>> {
        sqlx::query(
            r#"
            INSERT INTO saga_timers
            (timer_id, saga_id, run_id, fire_at, timer_type, status,
             attributes, attempt, max_attempts, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&timer.timer_id)
        .bind(&timer.saga_id.0)
        .bind(&timer.run_id)
        .bind(timer.fire_at.timestamp())
        .bind(timer.timer_type.as_str())
        .bind(format!("{:?}", timer.status))
        .bind(&timer.attributes)
        .bind(timer.attempt as i64)
        .bind(timer.max_attempts as i64)
        .bind(timer.created_at.timestamp())
        .execute(&*self.pool)
        .await
        .map_err(|e| TimerStoreError::Create(SqliteTimerStoreError::Backend(e)))?;

        Ok(())
    }

    async fn cancel_timer(&self, timer_id: &str) -> Result<(), TimerStoreError<Self::Error>> {
        sqlx::query(
            r#"
            UPDATE saga_timers
            SET status = 'cancelled'
            WHERE timer_id = ? AND status = 'pending'
            "#,
        )
        .bind(timer_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| TimerStoreError::Cancel(SqliteTimerStoreError::from(e)))?;

        Ok(())
    }

    async fn get_expired_timers(
        &self,
        limit: u64,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let rows: Vec<TimerRow> = sqlx::query_as::<_, TimerRow>(
            r#"
            SELECT timer_id, saga_id, run_id, fire_at, timer_type, status,
                   attributes, attempt, max_attempts, created_at
            FROM saga_timers
            WHERE fire_at <= ? AND status = 'pending'
            ORDER BY fire_at ASC
            LIMIT ?
            "#,
        )
        .bind(Utc::now().timestamp())
        .bind(limit as i64)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| TimerStoreError::Retrieve(SqliteTimerStoreError::Backend(e)))?;

          Ok(rows.into_iter().map(|row| row.into_timer()).collect())
    }

    async fn claim_timers(
        &self,
        timer_ids: &[String],
        _scheduler_id: &str,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        for timer_id in timer_ids {
            sqlx::query(
                r#"
                UPDATE saga_timers
                SET status = 'processing', attempt = attempt + 1
                WHERE timer_id = ? AND status = 'pending'
                "#,
            )
            .bind(timer_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| TimerStoreError::Update(SqliteTimerStoreError::Backend(e)))?;
        }

        if timer_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: String = timer_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query_str = format!(
            "SELECT timer_id, saga_id, run_id, fire_at, timer_type, status, attributes, attempt, max_attempts, created_at FROM saga_timers WHERE timer_id IN ({})",
            placeholders
        );

        let mut sqlx_query = sqlx::query_as::<_, TimerRow>(&query_str);
        for timer_id in timer_ids {
            sqlx_query = sqlx_query.bind(timer_id.as_str());
        }

        let rows = sqlx_query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| TimerStoreError::Retrieve(SqliteTimerStoreError::Backend(e)))?;

        Ok(rows.into_iter().map(|row| row.into_timer()).collect())
    }

    async fn update_timer_status(
        &self,
        timer_id: &str,
        status: TimerStatus,
    ) -> Result<(), TimerStoreError<Self::Error>> {
        sqlx::query(
            r#"
            UPDATE saga_timers
            SET status = ?
            WHERE timer_id = ?
            "#,
        )
        .bind(format!("{:?}", status))
        .bind(timer_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| TimerStoreError::Update(SqliteTimerStoreError::Backend(e)))?;

        Ok(())
    }

    async fn get_timers_for_saga(
        &self,
        saga_id: &SagaId,
        include_fired: bool,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let query = if include_fired {
            r#"
            SELECT timer_id, saga_id, run_id, fire_at, timer_type, status,
                   attributes, attempt, max_attempts, created_at
            FROM saga_timers
            WHERE saga_id = ?
            ORDER BY fire_at ASC
            "#
        } else {
            r#"
            SELECT timer_id, saga_id, run_id, fire_at, timer_type, status,
                   attributes, attempt, max_attempts, created_at
            FROM saga_timers
            WHERE saga_id = ? AND status != 'fired' AND status != 'cancelled'
            ORDER BY fire_at ASC
            "#
        };

        let rows: Vec<TimerRow> = sqlx::query_as::<_, TimerRow>(query)
            .bind(&saga_id.0)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| TimerStoreError::Retrieve(SqliteTimerStoreError::Backend(e)))?;

        Ok(rows.into_iter().map(|row| row.into_timer()).collect())
    }

    async fn get_timer(
        &self,
        timer_id: &str,
    ) -> Result<Option<DurableTimer>, TimerStoreError<Self::Error>> {
        let row = sqlx::query_as::<_, TimerRow>(
            r#"
            SELECT timer_id, saga_id, run_id, fire_at, timer_type, status,
                   attributes, attempt, max_attempts, created_at
            FROM saga_timers
            WHERE timer_id = ?
            "#,
        )
        .bind(timer_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| TimerStoreError::Retrieve(SqliteTimerStoreError::Backend(e)))?;

        Ok(row.map(|r| r.into_timer()))
    }
}

/// Internal row type for timer deserialization.
#[derive(sqlx::FromRow)]
struct TimerRow {
    timer_id: String,
    saga_id: String,
    run_id: String,
    fire_at: i64,
    timer_type: String,
    status: String,
    attributes: Vec<u8>,
    attempt: i64,
    max_attempts: i64,
    created_at: i64,
}

impl TimerRow {
    fn into_timer(self) -> DurableTimer {
        DurableTimer {
            timer_id: self.timer_id,
            saga_id: SagaId(self.saga_id),
            run_id: self.run_id,
            fire_at: DateTime::from_timestamp(self.fire_at, 0).unwrap_or(Utc::now()),
            timer_type: self.timer_type.parse().unwrap_or(TimerType::Custom("unknown".to_string())),
            status: self.status.parse().unwrap_or(TimerStatus::Pending),
            attributes: self.attributes,
            attempt: self.attempt as u32,
            max_attempts: self.max_attempts as u32,
            created_at: DateTime::from_timestamp(self.created_at, 0).unwrap_or(Utc::now()),
        }
    }
}

/// Errors from [`SqliteTimerStore`] operations.
#[derive(Debug, Error)]
pub enum SqliteTimerStoreError {
    /// Backend-specific error.
    #[error("Backend error: {0}")]
    Backend(#[from] sqlx::Error),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Missing path for file-based storage.
    #[error("Path is required for file-based storage")]
    MissingPath,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_timer_store_creation() {
        let _store = SqliteTimerStore::in_memory().await.unwrap();
    }
}
