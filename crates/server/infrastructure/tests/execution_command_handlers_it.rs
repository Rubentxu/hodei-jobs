//! Integration test for Execution Command Handlers via NATS
//!
//! This test verifies that:
//! 1. Commands published to NATS are picked up by the consumers.
//! 2. The appropriate handler logic is executed.
//! 3. The system state (DB) is updated accordingly.
//!
//! Focuses on:
//! - AssignWorkerCommand
//! - ExecuteJobCommand
//! - CompleteJobCommand

use anyhow::Context;

use hodei_server_domain::command::{Command, CommandMetadataDefault};
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::commands::execution::{
    AssignWorkerCommand, AssignWorkerHandler, CompleteJobCommand, CompleteJobHandler,
    ExecuteJobCommand, ExecuteJobHandler,
};
use hodei_server_domain::shared_kernel::{JobId, JobState, WorkerId, WorkerState};
use hodei_server_domain::workers::WorkerRegistry;
use hodei_server_infrastructure::persistence::postgres::{
    PostgresJobRepository, PostgresWorkerRegistry,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

async fn get_postgres_pool() -> Result<sqlx::PgPool, sqlx::Error> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
}

async fn get_nats_client() -> anyhow::Result<async_nats::Client> {
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    async_nats::connect(nats_url)
        .await
        .context("Failed to connect to NATS")
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and NATS"]
async fn test_assign_worker_command_flow() -> anyhow::Result<()> {
    // 1. Setup
    let pool = get_postgres_pool().await?;
    let nats = get_nats_client().await?;

    let job_repo = Arc::new(PostgresJobRepository::new(pool.clone()));
    let worker_repo = Arc::new(PostgresWorkerRegistry::new(pool.clone()));

    // Create test data
    let job_id = JobId::new();
    let worker_id = WorkerId::new();
    let saga_id = format!("saga-test-{}", Uuid::new_v4());

    // Insert Job (Pending)
    sqlx::query(
        "INSERT INTO hodei_jobs (id, state, created_at, updated_at, request, execution_history)
         VALUES ($1, 'PENDING', NOW(), NOW(), '{}', '[]')",
    )
    .bind(job_id.to_string())
    .execute(&pool)
    .await?;

    // Insert Worker (Ready)
    sqlx::query(
        "INSERT INTO hodei_workers (id, state, last_heartbeat, capabilities, current_load, version, status)
         VALUES ($1, 'READY', NOW(), '[]', 0, '1.0.0', 'ONLINE')"
    )
    .bind(worker_id.to_string())
    .execute(&pool)
    .await?;

    // 2. Start Consumer
    hodei_server_infrastructure::messaging::execution_command_consumers::start_execution_command_consumers(
       nats.clone(), job_repo.clone(), worker_repo.clone(), None
    ).await?;

    // Simulate NATS subscription receiving
    let subject = "hodei.commands.worker.assignworker";
    let command =
        AssignWorkerCommand::with_worker(job_id.clone(), worker_id.clone(), saga_id.clone());

    // Publish to NATS
    let payload = serde_json::to_vec(&command)?;
    nats.publish(subject.to_string(), payload.into()).await?;

    // Wait for processing (background consumer needs time to pick up message)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // 3. Verify State
    let job = job_repo.find_by_id(&job_id).await?.unwrap();
    let worker = worker_repo.find_by_id(&worker_id).await?.unwrap();

    assert_eq!(*job.state(), JobState::Assigned, "Job should be ASSIGNED");
    assert_eq!(*worker.state(), WorkerState::Busy, "Worker should be BUSY");

    Ok(())
}
