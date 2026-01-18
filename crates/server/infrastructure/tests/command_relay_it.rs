//! Integration test for CommandRelay
//!
//! This test verifies:
//! 1. Command processing pipeline
//! 2. NATS publication
//! 3. Database status updates
//!
//! Requires running PostgreSQL and NATS.

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use hodei_server_domain::command::{
    CommandOutboxInsert, CommandOutboxRepository, CommandOutboxStatus, CommandTargetType,
};
use hodei_server_infrastructure::messaging::hybrid::command_relay::{
    CommandRelay, CommandRelayConfig,
};
use hodei_server_infrastructure::persistence::command_outbox::PostgresCommandOutboxRepository;
use sqlx::postgres::PgPoolOptions;

/// Helper to check if Postgres is available
async fn get_postgres_pool() -> Result<sqlx::PgPool, sqlx::Error> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(2))
        .connect(&db_url)
        .await
}

/// Helper to get NATS client
async fn get_nats_client() -> Result<async_nats::Client, async_nats::Error> {
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    async_nats::connect(nats_url).await
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and NATS"]
async fn test_command_relay_processing() -> anyhow::Result<()> {
    // 1. Setup dependencies
    let pool = get_postgres_pool().await?;
    let nats_client = Arc::new(get_nats_client().await?);
    let repo = Arc::new(PostgresCommandOutboxRepository::new(pool.clone()));

    // 2. Setup Schema (ensure table exists)
    // Note: In real env migrations are run via CLI, here we might assume they ran or use repo helper if any
    // PostgresCommandOutboxRepository has run_migrations in its impl? Yes.
    repo.run_migrations().await?;

    // 3. Create Relay
    let config = CommandRelayConfig {
        batch_size: 10,
        poll_interval_ms: 50,
        channel: "test_commands".to_string(),
        max_retries: 3,
        circuit_breaker_failure_threshold: 5,
        circuit_breaker_open_duration: Duration::from_secs(5),
        circuit_breaker_success_threshold: 2,
        circuit_breaker_call_timeout: Duration::from_secs(5),
    };

    let (relay, _shutdown_tx) =
        CommandRelay::new(&pool, repo.clone(), nats_client, Some(config)).await?;

    // Start relay in background
    let relay_handle = tokio::spawn(async move {
        relay.run().await;
    });

    // 4. Insert Command
    let command_id = Uuid::new_v4();
    let command = CommandOutboxInsert::new(
        Uuid::new_v4(),
        CommandTargetType::Worker,
        "TestCommand".to_string(),
        serde_json::json!({"test": true}),
        None,
        Some(format!("test-cmd-{}", command_id)),
    );

    repo.insert_command(&command).await?;

    // 5. Wait for processing (Polling interval 50ms)
    time::sleep(Duration::from_millis(500)).await;

    // 6. Verify processing status
    // Use repo internal method or query if necessary?
    // CommandOutboxRepository trait has no find_by_id?
    // We can use get_stats or query DB directly.

    // Check pending count should be 0, completed 1
    let stats = repo.get_stats().await?;
    assert_eq!(stats.pending_count, 0, "Command should not be pending");
    assert_eq!(stats.completed_count, 1, "Command should be completed");

    // Cleanup
    relay_handle.abort(); // Simplify shutdown for test
    Ok(())
}

use tokio::time;
