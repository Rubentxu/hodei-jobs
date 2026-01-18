//! Transactional Worker Registry Implementation
//!
//! PostgreSQL implementation of WorkerRegistryTx.

use super::PostgresWorkerRegistry;
use async_trait::async_trait;
use hodei_server_domain::shared_kernel::{DomainError, Result, WorkerId, WorkerState};
use hodei_server_domain::workers::{Worker, registry::WorkerRegistryTx};
use sqlx::{PgTransaction, Row};

#[async_trait]
impl WorkerRegistryTx for PostgresWorkerRegistry {
    async fn save_with_tx(&self, tx: &mut PgTransaction<'_>, worker: &Worker) -> Result<()> {
        let handle = worker.handle();
        let spec = worker.spec();
        let worker_id = handle.worker_id.0;
        let provider_id = handle.provider_id.0;
        let current_job_id = worker.current_job_id().map(|j| j.0);

        sqlx::query(
            r#"
            INSERT INTO workers (id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) DO UPDATE SET
                provider_id = EXCLUDED.provider_id,
                provider_type = EXCLUDED.provider_type,
                provider_resource_id = EXCLUDED.provider_resource_id,
                state = EXCLUDED.state,
                spec = EXCLUDED.spec,
                handle = EXCLUDED.handle,
                current_job_id = EXCLUDED.current_job_id,
                last_heartbeat = EXCLUDED.last_heartbeat,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(worker_id)
        .bind(provider_id)
        .bind(handle.provider_type.to_string())
        .bind(handle.provider_resource_id.clone())
        .bind(worker.state().to_string())
        .bind(serde_json::to_value(spec).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to serialize worker spec: {}", e),
        })?)
        .bind(serde_json::to_value(handle).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to serialize worker handle: {}", e),
        })?)
        .bind(current_job_id)
        .bind(worker.last_heartbeat())
        .bind(worker.created_at())
        .bind(worker.updated_at())
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save worker in transaction: {}", e),
        })?;

        Ok(())
    }

    async fn update_state_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        worker_id: &WorkerId,
        state: WorkerState,
    ) -> Result<()> {
        let worker_uuid = worker_id.0;

        sqlx::query("UPDATE workers SET state = $1, updated_at = NOW() WHERE id = $2")
            .bind(state.to_string())
            .bind(worker_uuid)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to update worker state in transaction: {}", e),
            })?;

        Ok(())
    }

    async fn release_from_job_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        worker_id: &WorkerId,
    ) -> Result<()> {
        let worker_uuid = worker_id.0;

        sqlx::query(
            "UPDATE workers SET current_job_id = NULL, state = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(WorkerState::Ready.to_string())
        .bind(worker_uuid)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to release worker from job in transaction: {}", e),
        })?;

        Ok(())
    }

    async fn register_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        handle: hodei_server_domain::workers::WorkerHandle,
        spec: hodei_server_domain::workers::WorkerSpec,
        job_id: hodei_server_domain::shared_kernel::JobId,
    ) -> Result<Worker> {
        let worker_id = handle.worker_id.0;
        let provider_id = handle.provider_id.0;
        let current_job_id = job_id.0;
        let worker_name = format!("hodei-worker-{}", &worker_id.to_string()[0..8]);

        sqlx::query(
            r#"
            INSERT INTO workers (id, name, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                provider_id = EXCLUDED.provider_id,
                provider_type = EXCLUDED.provider_type,
                provider_resource_id = EXCLUDED.provider_resource_id,
                state = EXCLUDED.state,
                spec = EXCLUDED.spec,
                handle = EXCLUDED.handle,
                current_job_id = EXCLUDED.current_job_id,
                updated_at = NOW()
            "#,
        )
        .bind(worker_id)
        .bind(&worker_name)
        .bind(provider_id)
        .bind(handle.provider_type.to_string())
        .bind(handle.provider_resource_id.clone())
        .bind(WorkerState::Creating.to_string())
        .bind(serde_json::to_value(&spec).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to serialize worker spec: {}", e),
        })?)
        .bind(serde_json::to_value(&handle).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to serialize worker handle: {}", e),
        })?)
        .bind(current_job_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to register worker in transaction: {}", e),
        })?;

        Ok(Worker::new(handle, spec))
    }

    async fn unregister_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        worker_id: &WorkerId,
    ) -> Result<()> {
        let worker_uuid = worker_id.0;

        sqlx::query("DELETE FROM workers WHERE id = $1")
            .bind(worker_uuid)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to unregister worker in transaction: {}", e),
            })?;

        Ok(())
    }

    async fn find_available_with_tx(&self, tx: &mut PgTransaction<'_>) -> Result<Vec<Worker>> {
        let rows = sqlx::query(
            r#"
            SELECT id, provider_id, provider_type, provider_resource_id, state, spec, handle, current_job_id, last_heartbeat, created_at, updated_at
            FROM workers
            WHERE state = $1 AND current_job_id IS NULL
            "#,
        )
        .bind(WorkerState::Ready.to_string())
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find available workers in transaction: {}", e),
        })?;

        let mut workers = Vec::new();
        for row in rows {
            workers.push(super::worker_registry::map_row_to_worker(row)?);
        }

        Ok(workers)
    }
}
