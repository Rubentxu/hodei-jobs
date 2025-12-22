use futures::StreamExt;
use hodei_server_application::jobs::{CreateJobRequest, CreateJobUseCase, JobSpecRequest};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::{JobQueue, JobRepository};
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use hodei_server_infrastructure::persistence::postgres::{PostgresJobQueue, PostgresJobRepository};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

#[tokio::test]
async fn test_create_job_atomic_persistence_and_event() {
    // 1. Setup Postgres
    let node = Postgres::default()
        .start()
        .await
        .expect("Failed to start Postgres");
    let connection_string = format!(
        "postgres://postgres:postgres@127.0.0.1:{}/postgres",
        node.get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port")
    );

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .expect("Failed to connect to DB");

    // 2. Initialize Infrastructure
    let job_repo = Arc::new(PostgresJobRepository::new(pool.clone()));
    let job_queue = Arc::new(PostgresJobQueue::new(pool.clone()));
    let event_bus = Arc::new(PostgresEventBus::new(pool.clone()));

    job_repo
        .run_migrations()
        .await
        .expect("Failed to migrate jobs");
    job_queue
        .run_migrations()
        .await
        .expect("Failed to migrate queue");
    event_bus
        .run_migrations()
        .await
        .expect("Failed to migrate domain events");

    // 3. Initialize Use Case
    let use_case = CreateJobUseCase::new(job_repo.clone(), job_queue.clone(), event_bus.clone());

    // 4. Subscribe to events
    let mut event_stream = event_bus
        .subscribe("hodei_events")
        .await
        .expect("Failed to subscribe");

    // 5. Execute Create Job
    let request = CreateJobRequest {
        spec: JobSpecRequest {
            command: vec!["echo".to_string(), "test".to_string()],
            image: Some("alpine:latest".to_string()),
            env: None,
            timeout_ms: Some(5000),
            working_dir: None,
            cpu_cores: None,
            memory_bytes: None,
            disk_bytes: None,
        },
        correlation_id: Some("test-correlation".to_string()),
        actor: Some("test-actor".to_string()),
        job_id: None,
    };

    let response = use_case
        .execute(request)
        .await
        .expect("Failed to create job");
    let job_id = response.job_id;

    // 6. Verify Persistence (Atomic Check)
    // Check Jobs Table
    let job_in_repo = job_repo
        .find_by_id(&hodei_server_domain::shared_kernel::JobId(
            uuid::Uuid::parse_str(&job_id).unwrap(),
        ))
        .await
        .expect("Repo query failed");
    assert!(job_in_repo.is_some(), "Job should persist in repository");

    // Check Job Queue Table explicitly (via SQL query to ensure it is really there)
    // We cannot use job_queue.dequeue() because it changes state.
    // Use len() or sqlx query.
    let queue_len = job_queue.len().await.expect("Queue len failed");
    assert_eq!(queue_len, 1, "Queue should have 1 job");

    // 7. Verify Event
    let event = tokio::time::timeout(Duration::from_secs(2), event_stream.next())
        .await
        .expect("Timed out waiting for event")
        .expect("Stream ended");

    match event {
        Ok(DomainEvent::JobCreated {
            job_id: ev_id,
            correlation_id,
            ..
        }) => {
            assert_eq!(ev_id.to_string(), job_id, "Event job_id mismatch");
            assert_eq!(correlation_id, Some("test-correlation".to_string()));
        }
        _ => panic!("Expected JobCreated event, got {:?}", event),
    }
}
