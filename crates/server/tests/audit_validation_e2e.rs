//! E2E Tests with Audit Validation
//!
//! Story 15.6: Refactorizar Tests E2E con Validación Audit
//!
//! These tests demonstrate the pattern of validating audit logs in E2E tests.
//! They use unique correlation_ids per test and verify the expected event sequence.

mod common;

use std::sync::Arc;
use std::time::Duration;

use hodei_jobs_application::audit_test_helper::DbAuditTestHelper;
use hodei_jobs_domain::audit::AuditRepository;
use hodei_jobs_infrastructure::persistence::postgres::PostgresAuditRepository;
use uuid::Uuid;

/// Helper to create audit infrastructure for tests
async fn setup_audit_repository(pool: &sqlx::PgPool) -> anyhow::Result<Arc<dyn AuditRepository>> {
    let audit_repo_impl = PostgresAuditRepository::new(pool.clone());
    audit_repo_impl.run_migrations().await?;
    Ok(Arc::new(audit_repo_impl) as Arc<dyn AuditRepository>)
}

/// Test that demonstrates the AuditTestHelper pattern for validating event sequences
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_audit_helper_event_sequence_validation() {
    // Setup: Get isolated test database
    let db = common::get_postgres_context().await.unwrap();
    let pool = sqlx::PgPool::connect(&db.connection_string).await.unwrap();

    // Create audit repository
    let audit_repository = setup_audit_repository(&pool).await.unwrap();

    // Create audit test helper
    let audit_helper = DbAuditTestHelper::new(audit_repository.clone());

    // Clear any existing logs
    audit_helper.clear().await.unwrap();

    // Generate unique correlation_id for this test
    let correlation_id = format!("test-sequence-{}", Uuid::new_v4());

    // Simulate a job lifecycle by inserting logs directly
    let log1 = hodei_jobs_domain::audit::AuditLog::new(
        "JobCreated".to_string(),
        serde_json::json!({"job_id": "test-job-1"}),
        Some(correlation_id.clone()),
        Some("test-actor".to_string()),
    );
    audit_repository.save(&log1).await.unwrap();

    // Small delay to ensure different timestamps
    tokio::time::sleep(Duration::from_millis(10)).await;

    let log2 = hodei_jobs_domain::audit::AuditLog::new(
        "JobStatusChanged".to_string(),
        serde_json::json!({"job_id": "test-job-1", "new_state": "Running"}),
        Some(correlation_id.clone()),
        Some("test-actor".to_string()),
    );
    audit_repository.save(&log2).await.unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;

    let log3 = hodei_jobs_domain::audit::AuditLog::new(
        "JobStatusChanged".to_string(),
        serde_json::json!({"job_id": "test-job-1", "new_state": "Completed"}),
        Some(correlation_id.clone()),
        Some("test-actor".to_string()),
    );
    audit_repository.save(&log3).await.unwrap();

    // Verify event sequence using assert_event_sequence
    let result = audit_helper
        .assert_event_sequence(
            &correlation_id,
            &["JobCreated", "JobStatusChanged"],
            Duration::from_secs(5),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected event sequence to match: {:?}",
        result
    );
}

/// Test that demonstrates wait_for_audit_log with timeout
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_audit_helper_wait_for_log() {
    // Setup
    let db = common::get_postgres_context().await.unwrap();
    let pool = sqlx::PgPool::connect(&db.connection_string).await.unwrap();

    let audit_repository = setup_audit_repository(&pool).await.unwrap();
    let audit_helper = DbAuditTestHelper::new(audit_repository.clone());

    audit_helper.clear().await.unwrap();

    let correlation_id = format!("test-wait-{}", Uuid::new_v4());

    // Spawn a task that will insert a log after a delay
    let repo_clone = audit_repository.clone();
    let corr_clone = correlation_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let log = hodei_jobs_domain::audit::AuditLog::new(
            "WorkerRegistered".to_string(),
            serde_json::json!({"worker_id": "test-worker"}),
            Some(corr_clone),
            Some("test-actor".to_string()),
        );
        repo_clone.save(&log).await.unwrap();
    });

    // Wait for the log with timeout
    let result = audit_helper
        .wait_for_audit_log(&correlation_id, "WorkerRegistered", Duration::from_secs(5))
        .await;

    assert!(result.is_ok(), "Expected to find WorkerRegistered log");
    let log = result.unwrap();
    assert_eq!(log.event_type, "WorkerRegistered");
}

/// Test that demonstrates assert_no_events
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_audit_helper_assert_no_events() {
    // Setup
    let db = common::get_postgres_context().await.unwrap();
    let pool = sqlx::PgPool::connect(&db.connection_string).await.unwrap();

    let audit_repository = setup_audit_repository(&pool).await.unwrap();
    let audit_helper = DbAuditTestHelper::new(audit_repository.clone());

    audit_helper.clear().await.unwrap();

    // Use a unique correlation_id that has no events
    let correlation_id = format!("test-no-events-{}", Uuid::new_v4());

    // Verify no events exist for this correlation_id
    let result = audit_helper.assert_no_events(&correlation_id).await;
    assert!(result.is_ok(), "Expected no events for new correlation_id");

    // Now add an event
    let log = hodei_jobs_domain::audit::AuditLog::new(
        "TestEvent".to_string(),
        serde_json::json!({}),
        Some(correlation_id.clone()),
        None,
    );
    audit_repository.save(&log).await.unwrap();

    // Now assert_no_events should fail
    let result = audit_helper.assert_no_events(&correlation_id).await;
    assert!(result.is_err(), "Expected error when events exist");
}

/// Test that demonstrates find_by_event_type
#[tokio::test]
#[ignore = "Requires Docker and Testcontainers"]
async fn test_audit_helper_find_by_event_type() {
    // Setup
    let db = common::get_postgres_context().await.unwrap();
    let pool = sqlx::PgPool::connect(&db.connection_string).await.unwrap();

    let audit_repository = setup_audit_repository(&pool).await.unwrap();
    let audit_helper = DbAuditTestHelper::new(audit_repository.clone());

    audit_helper.clear().await.unwrap();

    // Insert various event types
    for i in 0..5 {
        let log = hodei_jobs_domain::audit::AuditLog::new(
            "JobCreated".to_string(),
            serde_json::json!({"job_id": format!("job-{}", i)}),
            Some(format!("corr-{}", i)),
            Some("test-actor".to_string()),
        );
        audit_repository.save(&log).await.unwrap();
    }

    for i in 0..3 {
        let log = hodei_jobs_domain::audit::AuditLog::new(
            "WorkerRegistered".to_string(),
            serde_json::json!({"worker_id": format!("worker-{}", i)}),
            Some(format!("worker-corr-{}", i)),
            Some("test-actor".to_string()),
        );
        audit_repository.save(&log).await.unwrap();
    }

    // Find JobCreated events
    let job_logs = audit_helper
        .find_by_event_type("JobCreated", 10)
        .await
        .unwrap();
    assert_eq!(job_logs.len(), 5, "Expected 5 JobCreated events");

    // Find WorkerRegistered events
    let worker_logs = audit_helper
        .find_by_event_type("WorkerRegistered", 10)
        .await
        .unwrap();
    assert_eq!(worker_logs.len(), 3, "Expected 3 WorkerRegistered events");

    // Find non-existent event type
    let empty_logs = audit_helper
        .find_by_event_type("NonExistent", 10)
        .await
        .unwrap();
    assert!(empty_logs.is_empty(), "Expected no NonExistent events");
}
