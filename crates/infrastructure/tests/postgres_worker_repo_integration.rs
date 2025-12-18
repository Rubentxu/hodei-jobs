use hodei_jobs_domain::{
    shared_kernel::{ProviderId, WorkerId, WorkerState},
    workers::{ProviderType, WorkerHandle, WorkerSpec, registry::WorkerRegistry},
};
use hodei_jobs_infrastructure::persistence::{DatabaseConfig, PostgresWorkerRegistry};

mod common;

#[tokio::test]
#[ignore = "Requires Docker with PostgreSQL"]
async fn test_postgres_worker_lifecycle() {
    let _db = common::get_postgres_context().await;

    let config = DatabaseConfig {
        url: _db.connection_string.clone(),
        max_connections: 5,
        connection_timeout: std::time::Duration::from_secs(30),
    };

    let registry = PostgresWorkerRegistry::connect(&config)
        .await
        .expect("Failed to connect to Postgres");

    registry
        .run_migrations()
        .await
        .expect("Failed to run migrations");

    // 1. Create Handle and Spec
    let worker_id = WorkerId::new();
    let provider_id = ProviderId::new();
    let provider_type = ProviderType::Docker;

    let handle = WorkerHandle::new(
        worker_id.clone(),
        "container-123".to_string(),
        provider_type.clone(),
        provider_id.clone(),
    );

    let spec = WorkerSpec::new(
        "hodei-jobs-worker:latest".to_string(),
        "http://localhost:50051".to_string(),
    );

    // 2. Register
    let worker = registry
        .register(handle, spec)
        .await
        .expect("Failed to register worker");
    assert_eq!(*worker.id(), worker_id);
    assert_eq!(*worker.state(), WorkerState::Creating);

    // 3. Get by ID
    let found = registry
        .get(&worker_id)
        .await
        .expect("Failed to get worker");
    assert!(found.is_some());
    let found_worker = found.unwrap();
    assert_eq!(*found_worker.id(), worker_id);

    // 4. Update State (Connecting -> Ready)
    registry
        .update_state(&worker_id, WorkerState::Connecting)
        .await
        .expect("Failed to update state");
    let w = registry.get(&worker_id).await.expect("Get failed").unwrap();
    assert_eq!(*w.state(), WorkerState::Connecting);

    registry
        .update_state(&worker_id, WorkerState::Ready)
        .await
        .expect("Failed to update state");
    let w = registry.get(&worker_id).await.expect("Get failed").unwrap();
    assert_eq!(*w.state(), WorkerState::Ready);

    // 5. Heartbeat
    let before_heartbeat = w.last_heartbeat();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await; // Ensure time passes
    registry
        .heartbeat(&worker_id)
        .await
        .expect("Failed to heartbeat");
    let w_after = registry.get(&worker_id).await.expect("Get failed").unwrap();
    assert!(w_after.last_heartbeat() > before_heartbeat);

    // 6. Find Available
    let available = registry
        .find_available()
        .await
        .expect("Failed to find available");
    assert!(available.iter().any(|w| *w.id() == worker_id));

    // 7. Unregister
    registry
        .unregister(&worker_id)
        .await
        .expect("Failed to unregister");
    let found_deleted = registry
        .get(&worker_id)
        .await
        .expect("Failed to get deleted");
    assert!(found_deleted.is_none());
}
