use chrono::Utc;
use futures::StreamExt;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobSpec;
use hodei_server_domain::shared_kernel::JobId;
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::time::timeout;

#[tokio::test]
async fn test_postgres_event_bus_pub_sub() {
    // Start Postgres container
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

    // Create Pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .expect("Failed to connect to DB");

    // Initialize EventBus
    let bus = PostgresEventBus::new(pool);

    // Subscribe
    let mut stream = bus
        .subscribe("hodei_events")
        .await
        .expect("Failed to subscribe");

    // Create Event
    let job_id = JobId::new();
    let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()]);
    let event = DomainEvent::JobCreated {
        job_id: job_id.clone(),
        spec,
        occurred_at: Utc::now(),
        correlation_id: None,
        actor: None,
    };

    // Publish
    bus.publish(&event).await.expect("Failed to publish");

    // Verify reception (with timeout)
    let received = timeout(Duration::from_secs(5), stream.next()).await;

    match received {
        Ok(Some(Ok(received_event))) => {
            assert_eq!(event, received_event);
        }
        Ok(Some(Err(e))) => panic!("Received error from bus: {:?}", e),
        Ok(None) => panic!("Stream ended unexpectedly"),
        Err(_) => panic!("Timed out waiting for event"),
    }
}
