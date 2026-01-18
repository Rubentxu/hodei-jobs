//! Integration test for PostgresEventBus using pg_notify
//!
//! This test verifies:
//! 1. Event publishing to Postgres via pg_notify
//! 2. Event subscription and real-time reception
//! 3. Event serialization/deserialization
//! 4. Latency < 100ms for event delivery
//!
//! These tests require a running PostgreSQL instance.
//! Set DATABASE_URL environment variable or use the default connection string.
//! Run with: DATABASE_URL="postgres://postgres:postgres@localhost:5432/postgres" cargo test --test event_bus_pg_notify_it -- --ignored

use std::time::{Duration, Instant};
use tokio::time::timeout;

use chrono::Utc;
use futures::StreamExt;
use hodei_server_domain::JobCreated;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobSpec;
use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use sqlx::postgres::PgPoolOptions;

/// Default channel used by PostgresEventBus
const HODEI_EVENTS_CHANNEL: &str = "hodei_events";

/// Helper to check if Postgres is available
async fn get_postgres_pool() -> Result<sqlx::PgPool, sqlx::Error> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());

    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(2))
        .connect(&db_url)
        .await
}

/// Test EventBus pg_notify integration with real Postgres
///
/// Requires: Running PostgreSQL instance
#[tokio::test]
#[ignore = "Requires running PostgreSQL - run with --ignored flag"]
async fn event_bus_publish_and_subscribe() -> anyhow::Result<()> {
    let pool = get_postgres_pool().await?;
    let event_bus = PostgresEventBus::new(pool);

    // Subscribe to the actual channel used by PostgresEventBus
    let mut subscriber = event_bus.subscribe(HODEI_EVENTS_CHANNEL).await?;

    // Small delay to ensure listener is ready
    tokio::time::sleep(Duration::from_millis(50)).await;

    let job_id = JobId::new();
    let event = DomainEvent::JobCreated(JobCreated {
        job_id: job_id.clone(),
        spec: JobSpec::new(vec!["echo".to_string(), "test".to_string()]),
        occurred_at: Utc::now(),
        correlation_id: None,
        actor: None,
    });
    event_bus.publish(&event).await?;

    let received_event = match timeout(Duration::from_secs(5), subscriber.next()).await {
        Ok(Some(Ok(event))) => event,
        Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Event error: {}", e)),
        Ok(None) => return Err(anyhow::anyhow!("Stream ended")),
        Err(_) => return Err(anyhow::anyhow!("Timeout waiting for event")),
    };

    assert_eq!(received_event.event_type(), "JobCreated");

    // Verify it's the same event we published
    if let DomainEvent::JobCreated(JobCreated {
        job_id: received_id,
        ..
    }) = received_event
    {
        assert_eq!(received_id, job_id);
    } else {
        panic!("Expected JobCreated event");
    }

    Ok(())
}

/// Test event delivery latency < 100ms
///
/// Requires: Running PostgreSQL instance
#[tokio::test]
#[ignore = "Requires running PostgreSQL - run with --ignored flag"]
async fn event_bus_latency_under_100ms() -> anyhow::Result<()> {
    let pool = get_postgres_pool().await?;
    let event_bus = PostgresEventBus::new(pool);

    let mut subscriber = event_bus.subscribe(HODEI_EVENTS_CHANNEL).await?;

    // Small delay to ensure listener is ready
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Measure latency for 5 events
    let mut latencies = Vec::new();
    for i in 0..5 {
        let event = DomainEvent::JobCreated(JobCreated {
            job_id: JobId::new(),
            spec: JobSpec::new(vec!["echo".to_string(), format!("test_{}", i)]),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        });
        let start = Instant::now();

        event_bus.publish(&event).await?;

        let received = match timeout(Duration::from_secs(5), subscriber.next()).await {
            Ok(Some(Ok(event))) => event,
            Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Event error: {}", e)),
            Ok(None) => return Err(anyhow::anyhow!("Stream ended")),
            Err(_) => return Err(anyhow::anyhow!("Timeout waiting for event")),
        };

        assert!(matches!(received, DomainEvent::JobCreated { .. }));
        latencies.push(start.elapsed());
    }

    let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    let max_latency = latencies.iter().max().copied().unwrap_or_default();

    println!("Average latency: {:?}", avg_latency);
    println!("Max latency: {:?}", max_latency);

    assert!(
        avg_latency < Duration::from_millis(100),
        "Average latency {}ms exceeded 100ms threshold",
        avg_latency.as_millis()
    );

    assert!(
        max_latency < Duration::from_millis(200),
        "Max latency {}ms exceeded 200ms threshold",
        max_latency.as_millis()
    );

    Ok(())
}

/// Test concurrent subscriptions
///
/// Note: pg_notify broadcasts to ALL listeners on the same channel.
/// This test verifies that multiple subscribers receive the same event.
///
/// Requires: Running PostgreSQL instance
#[tokio::test]
#[ignore = "Requires running PostgreSQL - run with --ignored flag"]
async fn event_bus_concurrent_subscribers() -> anyhow::Result<()> {
    let pool = get_postgres_pool().await?;
    let event_bus = PostgresEventBus::new(pool);

    let mut subscriber1 = event_bus.subscribe(HODEI_EVENTS_CHANNEL).await?;
    let mut subscriber2 = event_bus.subscribe(HODEI_EVENTS_CHANNEL).await?;
    let mut subscriber3 = event_bus.subscribe(HODEI_EVENTS_CHANNEL).await?;

    // Small delay to ensure all listeners are ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    let job_id = JobId::new();
    let event = DomainEvent::JobCreated(JobCreated {
        job_id: job_id.clone(),
        spec: JobSpec::new(vec!["echo".to_string(), "test".to_string()]),
        occurred_at: Utc::now(),
        correlation_id: None,
        actor: None,
    });
    event_bus.publish(&event).await?;

    // All subscribers should receive the same event
    let received1 = match timeout(Duration::from_secs(5), subscriber1.next()).await {
        Ok(Some(Ok(event))) => event,
        Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscriber 1 error: {}", e)),
        Ok(None) => return Err(anyhow::anyhow!("Stream 1 ended")),
        Err(_) => return Err(anyhow::anyhow!("Timeout subscriber 1")),
    };

    let received2 = match timeout(Duration::from_secs(5), subscriber2.next()).await {
        Ok(Some(Ok(event))) => event,
        Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscriber 2 error: {}", e)),
        Ok(None) => return Err(anyhow::anyhow!("Stream 2 ended")),
        Err(_) => return Err(anyhow::anyhow!("Timeout subscriber 2")),
    };

    let received3 = match timeout(Duration::from_secs(5), subscriber3.next()).await {
        Ok(Some(Ok(event))) => event,
        Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscriber 3 error: {}", e)),
        Ok(None) => return Err(anyhow::anyhow!("Stream 3 ended")),
        Err(_) => return Err(anyhow::anyhow!("Timeout subscriber 3")),
    };

    // All should receive JobCreated
    match (&received1, &received2, &received3) {
        (
            DomainEvent::JobCreated(JobCreated { job_id: id1, .. }),
            DomainEvent::JobCreated(JobCreated { job_id: id2, .. }),
            DomainEvent::JobCreated(JobCreated { job_id: id3, .. }),
        ) => {
            // All should have received the same job_id
            assert_eq!(*id1, job_id);
            assert_eq!(*id2, job_id);
            assert_eq!(*id3, job_id);
        }
        _ => panic!("Expected all JobCreated events"),
    }

    Ok(())
}

/// Test different event types are correctly published and received
///
/// Requires: Running PostgreSQL instance
#[tokio::test]
#[ignore = "Requires running PostgreSQL - run with --ignored flag"]
async fn event_bus_different_event_types() -> anyhow::Result<()> {
    let pool = get_postgres_pool().await?;
    let event_bus = PostgresEventBus::new(pool);

    let mut subscriber = event_bus.subscribe(HODEI_EVENTS_CHANNEL).await?;

    // Small delay to ensure listener is ready
    tokio::time::sleep(Duration::from_millis(50)).await;

    let events = vec![
        DomainEvent::JobCreated(JobCreated {
            job_id: JobId::new(),
            spec: JobSpec::new(vec!["echo".to_string(), "test".to_string()]),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        }),
        DomainEvent::WorkerRegistered {
            worker_id: WorkerId::new(),
            provider_id: ProviderId::new(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        },
        DomainEvent::JobAssigned {
            job_id: JobId::new(),
            worker_id: WorkerId::new(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        },
        DomainEvent::WorkerProvisioned {
            worker_id: WorkerId::new(),
            provider_id: ProviderId::new(),
            spec_summary: "test".to_string(),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        },
    ];

    // Publish all events
    for event in &events {
        event_bus.publish(event).await?;
        // Small delay between publishes
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut received_types = Vec::new();
    for _ in 0..events.len() {
        let received = match timeout(Duration::from_secs(5), subscriber.next()).await {
            Ok(Some(Ok(event))) => event,
            Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Event error: {}", e)),
            Ok(None) => return Err(anyhow::anyhow!("Stream ended")),
            Err(_) => return Err(anyhow::anyhow!("Timeout waiting for event")),
        };

        received_types.push(received.event_type().to_string());
    }

    let expected_types: Vec<String> = events.iter().map(|e| e.event_type().to_string()).collect();

    assert_eq!(received_types, expected_types);

    Ok(())
}
