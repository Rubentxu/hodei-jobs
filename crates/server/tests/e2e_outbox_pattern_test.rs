//! E2E Tests for Transactional Outbox Pattern Integration
//!
//! These tests verify the complete Transactional Outbox Pattern implementation:
//! 1. Events are atomically stored in the outbox table
//! 2. Outbox relay processes events asynchronously
//! 3. Idempotency keys prevent duplicate event processing
//! 4. Trace context is properly propagated
//! 5. Event cleanup removes obsolete events
//!
//! Run: cargo test --test e2e_outbox_pattern_test -- --ignored --nocapture

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use hodei_server_domain::events::{DomainEvent, EventMetadata, TraceContext};
use hodei_server_domain::outbox::{OutboxError, OutboxRepository};
use hodei_server_domain::shared_kernel::{JobId, JobState, WorkerId};
use sqlx::postgres::PgPool;
use tokio::time::timeout;
use uuid::Uuid;

use hodei_jobs::{JobDefinition, JobStatus, QueueJobRequest, ResourceRequirements};
use tonic::transport::Channel;

mod common;

use common::TestStack;

// =============================================================================
// Test State Tracking
// =============================================================================

/// Tracks events received during the test
#[derive(Debug, Default)]
struct EventTracker {
    published_events: Arc<Mutex<Vec<DomainEvent>>>,
}

impl EventTracker {
    fn new() -> Self {
        Self {
            published_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn subscribe(&self) -> EventSubscription {
        EventSubscription {
            events: self.published_events.clone(),
        }
    }

    fn assert_event_received(&self, predicate: impl Fn(&DomainEvent) -> bool) {
        let events = self.published_events.lock().unwrap();
        assert!(
            events.iter().any(predicate),
            "Expected event not found. Received events: {:?}",
            events
        );
    }

    fn assert_no_events(&self) {
        let events = self.published_events.lock().unwrap();
        assert!(
            events.is_empty(),
            "Expected no events, but found: {:?}",
            events
        );
    }
}

struct EventSubscription {
    events: Arc<Mutex<Vec<DomainEvent>>>,
}

impl EventSubscription {
    async fn wait_for_event(&self, timeout_secs: u64) -> Option<DomainEvent> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout_secs {
            let events = self.events.lock().unwrap();
            if !events.is_empty() {
                return Some(events[0].clone());
            }
            drop(events);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        None
    }
}

// =============================================================================
// Test: Outbox Event Creation and Relay Processing
// =============================================================================

/// Test that job dispatch creates outbox events and relay processes them
#[tokio::test]
#[ignore = "Requires Docker and database"]
async fn test_outbox_event_creation_and_relay_processing() {
    println!("\n🧪 E2E Test: Outbox Event Creation and Relay Processing");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Start test stack with outbox enabled
    println!("📦 Starting TestStack with outbox enabled...");
    let stack = match TestStack::with_docker_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("⚠️  Skipping test: Failed to start TestStack: {}", e);
            return;
        }
    };
    println!("✓ TestStack started");

    // 2. Get database connection for direct outbox verification
    println!("🔗 Connecting to database...");
    let pool = stack.server.postgres_pool().await;
    println!("✓ Database connected");

    // 3. Create a test job via gRPC
    println!("📝 Creating test job via gRPC...");
    let mut job_client = stack.server.job_client().await.expect("Job client");

    let job_definition = JobDefinition {
        spec: Some(hodei_jobs::JobSpec {
            command: vec!["echo".to_string(), "test".to_string()],
            image: Some("alpine:latest".to_string()),
            env: HashMap::new(),
            working_dir: None,
            resources: Some(ResourceRequirements {
                cpu_millis: Some(100),
                memory_bytes: Some(64 * 1024 * 1024),
            }),
        }),
    };

    let queue_request = QueueJobRequest {
        definition: Some(job_definition),
        metadata: HashMap::new(),
    };

    let queue_response = job_client
        .queue_job(tonic::Request::new(queue_request))
        .await
        .expect("Queue job failed")
        .into_inner();

    let job_id_str = queue_response.job_id;
    println!("✓ Job created: {}", job_id_str);

    // 4. Verify outbox event was created
    println!("🔍 Verifying outbox event was created...");
    let job_id = Uuid::parse_str(&job_id_str).unwrap();

    let outbox_repo = stack
        .server
        .outbox_repository()
        .await
        .expect("Outbox repository");

    let pending_events = outbox_repo
        .get_pending_events(10, 3)
        .await
        .expect("Failed to get pending events");

    assert!(
        !pending_events.is_empty(),
        "Expected outbox events to be created"
    );

    // Find JobCreated event
    let job_created_event = pending_events
        .iter()
        .find(|e| e.event_type == "JobCreated")
        .expect("JobCreated event not found");

    println!("✓ Outbox event created: {}", job_created_event.event_type);
    println!("  - Event ID: {}", job_created_event.id);
    println!("  - Idempotency Key: {:?}", job_created_event.idempotency_key);

    // 5. Verify idempotency key is present
    assert!(
        job_created_event.idempotency_key.is_some(),
        "Expected idempotency key to be present"
    );
    println!("✓ Idempotency key present");

    // 6. Start outbox relay
    println!("🚀 Starting outbox relay...");
    let relay = stack
        .server
        .outbox_relay()
        .await
        .expect("Outbox relay");

    let relay_metrics = relay.get_metrics();
    println!("✓ Outbox relay started");
    println!("  Initial queue depth: {}", relay_metrics.current_queue_depth);

    // 7. Wait for relay to process events
    println!("⏳ Waiting for relay to process events (5 seconds)...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    let final_metrics = relay.get_metrics();
    println!("✓ Relay processed events");
    println!("  Total events published: {}", final_metrics.total_events_published);
    println!("  Total events failed: {}", final_metrics.total_events_failed);
    println!("  Processing rate: {:.2} events/sec", final_metrics.processing_rate_per_second);

    // 8. Verify events were published
    assert!(
        final_metrics.total_events_published > 0,
        "Expected events to be published"
    );

    // 9. Verify outbox table is empty (all events processed)
    println!("🔍 Verifying outbox table is empty...");
    let final_pending = outbox_repo
        .get_pending_events(10, 3)
        .await
        .expect("Failed to get pending events");

    assert!(
        final_pending.is_empty(),
        "Expected all events to be processed, but found {} pending",
        final_pending.len()
    );
    println!("✓ All events processed successfully");

    println!("✅ Test passed: Outbox event creation and relay processing");
}

// =============================================================================
// Test: Idempotency Key Prevents Duplicate Processing
// =============================================================================

/// Test that duplicate events with the same idempotency key are rejected
#[tokio::test]
#[ignore = "Requires Docker and database"]
async fn test_idempotency_key_prevents_duplicates() {
    println!("\n🧪 E2E Test: Idempotency Key Prevents Duplicates");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Start test stack
    println!("📦 Starting TestStack...");
    let stack = match TestStack::with_docker_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("⚠️  Skipping test: Failed to start TestStack: {}", e);
            return;
        }
    };
    println!("✓ TestStack started");

    // 2. Get outbox repository
    let outbox_repo = stack
        .server
        .outbox_repository()
        .await
        .expect("Outbox repository");

    // 3. Insert same event twice with same idempotency key
    println!("📝 Inserting duplicate events with same idempotency key...");
    let job_id = Uuid::new_v4();
    let idempotency_key = format!("test-duplicate-{}", Uuid::new_v4());

    use hodei_server_domain::outbox::{OutboxEventInsert, AggregateType};

    let event1 = OutboxEventInsert::for_job(
        job_id,
        "TestEvent".to_string(),
        serde_json::json!({"test": "data1"}),
        None,
        Some(idempotency_key.clone()),
    );

    let event2 = OutboxEventInsert::for_job(
        job_id,
        "TestEvent".to_string(),
        serde_json::json!({"test": "data2"}),
        None,
        Some(idempotency_key.clone()),
    );

    // First insert should succeed
    outbox_repo
        .insert_events(&[event1])
        .await
        .expect("First insert should succeed");
    println!("✓ First event inserted successfully");

    // Second insert should fail with duplicate key error
    let result = outbox_repo.insert_events(&[event2]).await;

    match result {
        Err(OutboxError::DuplicateIdempotencyKey(key)) => {
            println!("✓ Duplicate rejected as expected: {}", key);
            assert_eq!(key, idempotency_key);
        }
        _ => panic!("Expected DuplicateIdempotencyKey error, got: {:?}", result),
    }

    // 4. Verify only one event exists
    println!("🔍 Verifying only one event exists...");
    let pending = outbox_repo
        .get_pending_events(10, 3)
        .await
        .expect("Failed to get pending events");

    assert_eq!(
        pending.len(),
        1,
        "Expected exactly one event, found {}",
        pending.len()
    );

    println!("✓ Idempotency key correctly prevented duplicate");

    println!("✅ Test passed: Idempotency key prevents duplicates");
}

// =============================================================================
// Test: Trace Context Propagation
// =============================================================================

/// Test that trace context is properly stored and can be extracted
#[tokio::test]
#[ignore = "Requires Docker and database"]
async fn test_trace_context_propagation() {
    println!("\n🧪 E2E Test: Trace Context Propagation");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Start test stack
    println!("📦 Starting TestStack...");
    let stack = match TestStack::with_docker_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("⚠️  Skipping test: Failed to start TestStack: {}", e);
            return;
        }
    };
    println!("✓ TestStack started");

    // 2. Get outbox repository
    let outbox_repo = stack
        .server
        .outbox_repository()
        .await
        .expect("Outbox repository");

    // 3. Create event with trace context
    println!("📝 Creating event with trace context...");
    let job_id = Uuid::new_v4();
    let trace_id = Uuid::new_v4().to_string();
    let span_id = Uuid::new_v4().to_string();

    let trace_context = TraceContext::new(trace_id.clone(), span_id.clone());

    let event = OutboxEventInsert::for_job(
        job_id,
        "TestEvent".to_string(),
        serde_json::json!({"test": "data"}),
        Some(serde_json::json!({
            "trace_id": trace_id,
            "span_id": span_id,
        })),
        Some(format!("trace-test-{}", Uuid::new_v4())),
    );

    outbox_repo
        .insert_events(&[event])
        .await
        .expect("Failed to insert event");

    println!("✓ Event with trace context inserted");

    // 4. Verify trace context is stored
    println!("🔍 Verifying trace context is stored...");
    let pending = outbox_repo
        .get_pending_events(10, 3)
        .await
        .expect("Failed to get pending events");

    assert_eq!(pending.len(), 1);

    let stored_event = &pending[0];
    assert!(stored_event.metadata.is_some());

    let metadata = stored_event.metadata.as_ref().unwrap();
    let stored_trace_id = metadata.get("trace_id").unwrap().as_str().unwrap();

    assert_eq!(stored_trace_id, trace_id);
    println!("✓ Trace context correctly stored");
    println!("  - Trace ID: {}", stored_trace_id);

    println!("✅ Test passed: Trace context propagation");
}

// =============================================================================
// Test: Event Cleanup
// =============================================================================

/// Test that cleanup methods remove obsolete events
#[tokio::test]
#[ignore = "Requires Docker and database"]
async fn test_event_cleanup() {
    println!("\n🧪 E2E Test: Event Cleanup");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Start test stack
    println!("📦 Starting TestStack...");
    let stack = match TestStack::with_docker_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("⚠️  Skipping test: Failed to start TestStack: {}", e);
            return;
        }
    };
    println!("✓ TestStack started");

    // 2. Get outbox repository
    let outbox_repo = stack
        .server
        .outbox_repository()
        .await
        .expect("Outbox repository");

    // 3. Insert some test events
    println!("📝 Inserting test events...");
    for i in 0..5 {
        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            format!("TestEvent{}", i),
            serde_json::json!({"index": i}),
            None,
            Some(format!("cleanup-test-{}", i)),
        );
        outbox_repo.insert_events(&[event]).await.unwrap();
    }

    // Mark first 3 as published
    let pending = outbox_repo
        .get_pending_events(10, 3)
        .await
        .unwrap();

    let published_ids: Vec<_> = pending.iter().take(3).map(|e| e.id).collect();
    outbox_repo.mark_published(&published_ids).await.unwrap();

    println!("✓ Inserted 5 events, marked 3 as published");

    // 4. Wait for published events to be "old"
    println!("⏳ Waiting for events to age (2 seconds)...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 5. Run cleanup for published events
    println!("🧹 Running cleanup for published events...");
    let deleted = outbox_repo
        .cleanup_published_events(Duration::from_secs(1))
        .await
        .expect("Cleanup failed");

    println!("✓ Cleaned up {} published events", deleted);
    assert_eq!(deleted, 3, "Expected 3 published events to be deleted");

    // 6. Verify remaining events
    let remaining = outbox_repo
        .get_pending_events(10, 3)
        .await
        .unwrap();

    assert_eq!(
        remaining.len(),
        2,
        "Expected 2 unpublished events to remain"
    );

    println!("✓ Cleanup correctly removed obsolete events");

    println!("✅ Test passed: Event cleanup");
}

// =============================================================================
// Test: Outbox Relay Metrics
// =============================================================================

/// Test that outbox relay collects comprehensive metrics
#[tokio::test]
#[ignore = "Requires Docker and database"]
async fn test_outbox_relay_metrics() {
    println!("\n🧪 E2E Test: Outbox Relay Metrics");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Start test stack
    println!("📦 Starting TestStack...");
    let stack = match TestStack::with_docker_provider().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("⚠️  Skipping test: Failed to start TestStack: {}", e);
            return;
        }
    };
    println!("✓ TestStack started");

    // 2. Get relay and check initial metrics
    let relay = stack
        .server
        .outbox_relay()
        .await
        .expect("Outbox relay");

    let initial_metrics = relay.get_metrics();
    println!("✓ Initial metrics snapshot:");
    println!("  - Events published: {}", initial_metrics.total_events_published);
    println!("  - Events failed: {}", initial_metrics.total_events_failed);
    println!("  - Processing rate: {:.2} events/sec", initial_metrics.processing_rate_per_second);

    // 3. Create some events
    let outbox_repo = stack
        .server
        .outbox_repository()
        .await
        .expect("Outbox repository");

    println!("📝 Creating 10 test events...");
    for i in 0..10 {
        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            format!("MetricsTest{}", i),
            serde_json::json!({"index": i}),
            None,
            Some(format!("metrics-test-{}", i)),
        );
        outbox_repo.insert_events(&[event]).await.unwrap();
    }

    // 4. Wait for relay to process
    println!("⏳ Waiting for relay to process events (5 seconds)...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    // 5. Check updated metrics
    let final_metrics = relay.get_metrics();
    println!("✓ Final metrics snapshot:");
    println!("{}", final_metrics);

    // 6. Verify metrics were updated
    assert!(
        final_metrics.total_events_published >= 5,
        "Expected at least 5 events published, got {}",
        final_metrics.total_events_published
    );

    assert!(
        final_metrics.avg_publish_duration_ms() > 0.0,
        "Expected non-zero average publish duration"
    );

    assert!(
        final_metrics.processing_rate_per_second() > 0.0,
        "Expected non-zero processing rate"
    );

    println!("✓ Metrics correctly tracked");

    println!("✅ Test passed: Outbox relay metrics");
}
