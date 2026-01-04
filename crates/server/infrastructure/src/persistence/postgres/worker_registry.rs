//! PostgreSQL Worker Registry Implementation
//!
//! Implements the WorkerRegistry trait using PostgreSQL as the backend.

use sqlx::{Pool, Postgres, Row};

use super::DatabaseConfig;
use async_trait::async_trait;
use hodei_server_domain::shared_kernel::{
    DomainError, JobId, ProviderId, Result, WorkerId, WorkerState,
};
use hodei_server_domain::workers::{
    Worker, WorkerHandle, WorkerSpec,
    registry::{WorkerFilter, WorkerRegistry, WorkerRegistryStats},
};

/// PostgreSQL-backed Worker Registry
#[derive(Debug, Clone)]
pub struct PostgresWorkerRegistry {
    pool: Pool<Postgres>,
}

impl PostgresWorkerRegistry {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;
        Ok(Self { pool })
    }

    /// Run database migrations for the worker registry
    ///
    /// DEPRECATED: Migrations are now handled by the central MigrationService.
    /// This method is kept for backwards compatibility but does nothing.
    pub async fn run_migrations(&self) -> Result<()> {
        // Migrations are now handled by the central MigrationService
        // See: hodei_server_infrastructure::persistence::postgres::migrations::run_migrations
        Ok(())
    }
}

#[async_trait]
impl WorkerRegistry for PostgresWorkerRegistry {
    async fn register(
        &self,
        handle: WorkerHandle,
        spec: WorkerSpec,
        job_id: JobId,
    ) -> Result<Worker> {
        let worker_id = handle.worker_id.0;
        let provider_id = handle.provider_id.0;
        let current_job_id = job_id.0;

        sqlx::query(
            r#"
            INSERT INTO workers (id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(worker_id)
        .bind(provider_id)
        .bind(handle.provider_type.to_string())
        .bind(handle.provider_resource_id.clone())
        .bind(WorkerState::Creating.to_string())
        .bind(serde_json::to_value(&spec).map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to serialize worker spec: {}", e),
            }
        })?)
        .bind(serde_json::to_value(&handle).map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to serialize worker handle: {}", e),
            }
        })?)
        .bind(current_job_id)
        .execute(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to register worker: {}", e),
        })?;

        let worker = Worker::new(handle, spec);
        Ok(worker)
    }

    async fn unregister(&self, worker_id: &WorkerId) -> Result<()> {
        let worker_uuid = worker_id.0;

        sqlx::query("DELETE FROM workers WHERE id = $1")
            .bind(worker_uuid)
            .execute(&self.pool)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to unregister worker: {}", e),
                },
            )?;

        Ok(())
    }

    async fn get(&self, worker_id: &WorkerId) -> Result<Option<Worker>> {
        let worker_uuid = worker_id.0;

        let row = sqlx::query(
            r#"
            SELECT id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at
            FROM workers
            WHERE id = $1
            "#,
        )
        .bind(worker_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to get worker: {}", e),
        })?;

        if let Some(row) = row {
            Ok(Some(map_row_to_worker(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find(&self, filter: &WorkerFilter) -> Result<Vec<Worker>> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            r#"
            SELECT id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at
            FROM workers
            "#,
        );

        let mut has_where = false;
        if let Some(states) = &filter.states {
            if !states.is_empty() {
                if !has_where {
                    qb.push(" WHERE ");
                    has_where = true;
                } else {
                    qb.push(" AND ");
                }
                qb.push("state = ANY(");
                qb.push_bind(states.iter().map(|s| s.to_string()).collect::<Vec<_>>());
                qb.push(")");
            }
        }

        if let Some(provider_id) = &filter.provider_id {
            if !has_where {
                qb.push(" WHERE ");
                has_where = true;
            } else {
                qb.push(" AND ");
            }
            qb.push("provider_id = ");
            qb.push_bind(provider_id.0);
        }

        if let Some(provider_type) = &filter.provider_type {
            if !has_where {
                qb.push(" WHERE ");
                has_where = true;
                let _ = has_where; // suppress warning
            } else {
                qb.push(" AND ");
            }
            qb.push("provider_type = ");
            qb.push_bind(provider_type.to_string());
        }

        qb.push(" ORDER BY created_at DESC");

        let rows = qb.build().fetch_all(&self.pool).await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to find workers: {}", e),
            }
        })?;

        let mut workers = Vec::new();
        for row in rows {
            workers.push(map_row_to_worker(row)?);
        }

        Ok(workers)
    }

    async fn find_available(&self) -> Result<Vec<Worker>> {
        let rows = sqlx::query(
            r#"
            SELECT id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at
            FROM workers
            WHERE state = $1 AND current_job_id IS NULL
            "#,
        )
        .bind(WorkerState::Ready.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to find available workers: {}", e),
        })?;

        let mut workers = Vec::new();
        for row in rows {
            workers.push(map_row_to_worker(row)?);
        }

        Ok(workers)
    }

    async fn find_by_provider(&self, provider_id: &ProviderId) -> Result<Vec<Worker>> {
        let provider_uuid = provider_id.0;

        let rows = sqlx::query(
            r#"
            SELECT id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at
            FROM workers
            WHERE provider_id = $1
            "#,
        )
        .bind(provider_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to find workers by provider: {}", e),
        })?;

        let mut workers = Vec::new();
        for row in rows {
            workers.push(map_row_to_worker(row)?);
        }

        Ok(workers)
    }

    async fn update_state(&self, worker_id: &WorkerId, state: WorkerState) -> Result<()> {
        let worker_uuid = worker_id.0;

        sqlx::query("UPDATE workers SET state = $1, updated_at = NOW() WHERE id = $2")
            .bind(state.to_string())
            .bind(worker_uuid)
            .execute(&self.pool)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to update worker state: {}", e),
                },
            )?;

        Ok(())
    }

    async fn heartbeat(&self, worker_id: &WorkerId) -> Result<()> {
        let worker_uuid = worker_id.0;

        sqlx::query("UPDATE workers SET last_heartbeat = NOW(), updated_at = NOW() WHERE id = $1")
            .bind(worker_uuid)
            .execute(&self.pool)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to update worker heartbeat: {}", e),
                },
            )?;

        Ok(())
    }

    async fn assign_to_job(&self, worker_id: &WorkerId, job_id: JobId) -> Result<()> {
        let worker_uuid = worker_id.0;
        let job_uuid = job_id.0;

        sqlx::query(
            "UPDATE workers SET current_job_id = $1, state = $2, updated_at = NOW() WHERE id = $3",
        )
        .bind(job_uuid)
        .bind(WorkerState::Busy.to_string())
        .bind(worker_uuid)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to assign worker to job: {}", e),
            }
        })?;

        Ok(())
    }

    async fn release_from_job(&self, worker_id: &WorkerId) -> Result<()> {
        let worker_uuid = worker_id.0;

        sqlx::query(
            "UPDATE workers SET current_job_id = NULL, state = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(WorkerState::Terminated.to_string()) // EPIC-21: Workers terminate after job completion
        .bind(worker_uuid)
        .execute(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to release worker from job: {}", e),
        })?;

        Ok(())
    }

    async fn find_unhealthy(&self, _timeout: std::time::Duration) -> Result<Vec<Worker>> {
        // Optimized query: Uses index on (state, last_heartbeat) for efficient filtering
        // Only fetches workers that are in active states (Ready, Busy, Connecting)
        // Excludes already terminated workers to reduce result set
        let rows = sqlx::query(
            r#"
            SELECT id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at
            FROM workers
            WHERE last_heartbeat IS NOT NULL
              AND last_heartbeat < NOW() - ($1 || ' seconds')::INTERVAL
              AND state NOT IN ('TERMINATED', 'TERMINATING')
            ORDER BY last_heartbeat ASC
            LIMIT 100
            "#,
        )
        .bind(_timeout.as_secs() as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to find unhealthy workers: {}", e),
        })?;

        let mut workers = Vec::new();
        for row in rows {
            workers.push(map_row_to_worker(row)?);
        }

        Ok(workers)
    }

    async fn find_for_termination(&self) -> Result<Vec<Worker>> {
        // Optimized query: Batch-limited to prevent large result sets
        // Uses compound condition for Ready workers and stale workers
        let rows = sqlx::query(
            r#"
            SELECT id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at
            FROM workers
            WHERE state = $1
               OR (last_heartbeat IS NOT NULL AND last_heartbeat < NOW() - INTERVAL '10 minutes')
            ORDER BY updated_at ASC
            LIMIT 50
            "#,
        )
        .bind(WorkerState::Ready.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to find workers for termination: {}", e),
        })?;

        let mut workers = Vec::new();
        for row in rows {
            workers.push(map_row_to_worker(row)?);
        }

        Ok(workers)
    }

    async fn stats(&self) -> Result<WorkerRegistryStats> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total,
                COUNT(CASE WHEN state = 'READY' THEN 1 END) as ready,
                COUNT(CASE WHEN state = 'BUSY' THEN 1 END) as busy,
                COUNT(CASE WHEN state = 'IDLE' THEN 1 END) as idle,
                COUNT(CASE WHEN state = 'TERMINATING' THEN 1 END) as terminating
            FROM workers
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to get worker stats: {}", e),
            }
        })?;

        let total: i64 = row.try_get("total").unwrap_or(0);
        let ready: i64 = row.try_get("ready").unwrap_or(0);
        let busy: i64 = row.try_get("busy").unwrap_or(0);
        let idle: i64 = row.try_get("idle").unwrap_or(0);
        let terminating: i64 = row.try_get("terminating").unwrap_or(0);

        Ok(WorkerRegistryStats {
            total_workers: total as usize,
            ready_workers: ready as usize,
            busy_workers: busy as usize,
            idle_workers: idle as usize,
            terminating_workers: terminating as usize,
            workers_by_provider: std::collections::HashMap::new(),
            workers_by_type: std::collections::HashMap::new(),
        })
    }

    // EPIC-26 US-26.7: TTL-related methods - these use in-memory filtering
    // since the Worker aggregate methods perform the actual TTL checks
    async fn find_idle_timed_out(&self) -> Result<Vec<Worker>> {
        // Get all workers and filter in memory using Worker::is_idle_timeout()
        let all_workers = self.find(&WorkerFilter::new()).await?;
        Ok(all_workers
            .into_iter()
            .filter(|w| w.is_idle_timeout())
            .collect())
    }

    async fn find_lifetime_exceeded(&self) -> Result<Vec<Worker>> {
        // Get all workers and filter in memory using Worker::is_lifetime_exceeded()
        let all_workers = self.find(&WorkerFilter::new()).await?;
        Ok(all_workers
            .into_iter()
            .filter(|w| w.is_lifetime_exceeded())
            .collect())
    }

    async fn find_ttl_after_completion_exceeded(&self) -> Result<Vec<Worker>> {
        // Get all workers and filter in memory using Worker::is_ttl_after_completion_exceeded()
        let all_workers = self.find(&WorkerFilter::new()).await?;
        Ok(all_workers
            .into_iter()
            .filter(|w| w.is_ttl_after_completion_exceeded())
            .collect())
    }

    async fn count(&self) -> Result<usize> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM workers")
            .fetch_one(&self.pool)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to count workers: {}", e),
                },
            )?;

        let count: i64 = row.try_get("count").unwrap_or(0);
        Ok(count as usize)
    }
}

fn map_row_to_worker(row: sqlx::postgres::PgRow) -> Result<Worker> {
    let handle: WorkerHandle = serde_json::from_value(row.get("handle")).map_err(|e| {
        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to deserialize worker handle: {}", e),
        }
    })?;

    let spec: WorkerSpec = serde_json::from_value(row.get("spec")).map_err(|e| {
        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to deserialize worker spec: {}", e),
        }
    })?;

    let state_str: String = row.get("state");
    let state = match state_str.as_str() {
        "CREATING" => hodei_server_domain::shared_kernel::WorkerState::Creating,
        "CONNECTING" => hodei_server_domain::shared_kernel::WorkerState::Connecting,
        "READY" => hodei_server_domain::shared_kernel::WorkerState::Ready,
        "BUSY" => hodei_server_domain::shared_kernel::WorkerState::Busy,
        "DRAINING" => hodei_server_domain::shared_kernel::WorkerState::Draining,
        "TERMINATING" => hodei_server_domain::shared_kernel::WorkerState::Terminating,
        "TERMINATED" => hodei_server_domain::shared_kernel::WorkerState::Terminated,
        _ => {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Invalid worker state: {}", state_str),
                },
            );
        }
    };

    let current_job_id: Option<uuid::Uuid> = row.get("current_job_id");
    let last_heartbeat: Option<chrono::DateTime<chrono::Utc>> = row.get("last_heartbeat");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

    Ok(Worker::from_database(
        handle,
        spec,
        state,
        current_job_id.map(|id| hodei_server_domain::shared_kernel::JobId(id)),
        last_heartbeat,
        created_at,
        updated_at,
    ))
}
