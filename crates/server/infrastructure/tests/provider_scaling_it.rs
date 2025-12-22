use async_trait::async_trait;
use futures::StreamExt;
use hodei_server_application::providers::ProviderManager;
use hodei_server_application::workers::{ProvisioningResult, WorkerProvisioningService};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::shared_kernel::{ProviderId, Result}; // WorkerId is part of WorkerSpec usually or generated
use hodei_server_domain::workers::WorkerSpec;
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use hodei_server_infrastructure::persistence::postgres::PostgresJobQueue;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::time::timeout;

// Mock Definition
struct MockProvisioningService {
    available_providers: Vec<ProviderId>,
}

impl MockProvisioningService {
    fn new(available_providers: Vec<ProviderId>) -> Self {
        Self {
            available_providers,
        }
    }
}

#[async_trait]
impl WorkerProvisioningService for MockProvisioningService {
    async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
    ) -> Result<ProvisioningResult> {
        Ok(ProvisioningResult::new(
            spec.worker_id,
            "mock_otp_token".to_string(),
            provider_id.clone(),
        ))
    }

    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool> {
        Ok(self.available_providers.contains(provider_id))
    }

    fn default_worker_spec(&self, _provider_id: &ProviderId) -> Option<WorkerSpec> {
        Some(WorkerSpec::new(
            "mock-image".to_string(),
            "http://mock-url".to_string(),
        ))
    }

    async fn list_providers(&self) -> Result<Vec<ProviderId>> {
        Ok(self.available_providers.clone())
    }
}

#[tokio::test]
async fn test_provider_scaling_flow() {
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

    // Run migrations (if necessary, but for this test we might skip if not relying on DB schema for events/jobs extensively,
    // BUT PostgresJobQueue likely needs schema. We'll skip schema init for now assuming events don't strictly need it if we trust EventBus just uses pg_notify?
    // Actually PostgresEventBus uses pg_notify, valid for empty DB.
    // PostgresJobQueue might expect tables. Let's see if we fail on JobQueue init.)
    // For now we assume we might need migrations if we use JobQueue strictly.
    // But we are testing ProviderManager which uses JobQueue reference.

    // 2. Setup EventBus
    let event_bus = PostgresEventBus::new(pool.clone());

    // Run migrations
    event_bus
        .run_migrations()
        .await
        .expect("Failed to run domain_events migrations");

    // Subscribe to events
    let mut event_stream = event_bus
        .subscribe("hodei_events")
        .await
        .expect("Failed to subscribe");

    // 3. Setup ProviderManager
    let job_queue = PostgresJobQueue::new(pool.clone());

    // We provide a mock provider to ensure scaling logic triggers
    let provider_id = ProviderId::new();
    let provisioning_service =
        std::sync::Arc::new(MockProvisioningService::new(vec![provider_id.clone()]));

    let event_bus_arc = std::sync::Arc::new(event_bus.clone());

    let manager = ProviderManager::new(
        event_bus_arc.clone(),
        provisioning_service,
        std::sync::Arc::new(job_queue),
    );

    manager
        .subscribe_to_events()
        .await
        .expect("Failed to subscribe manager");

    // 4. Publish JobQueueDepthChanged Event
    let depth_event = DomainEvent::JobQueueDepthChanged {
        queue_depth: 10,
        threshold: 5,
        occurred_at: chrono::Utc::now(),
        correlation_id: Some("test-correlation".to_string()),
        actor: None,
    };

    println!("Publishing JobQueueDepthChanged event...");
    event_bus
        .publish(&depth_event)
        .await
        .expect("Failed to publish depth event");

    // 5. Verification
    println!("Waiting for AutoScalingTriggered event...");

    let mut received_target = None;
    // Loop a few times to give async tasks time to process
    for _ in 0..10 {
        if let Ok(Some(Ok(evt))) = timeout(Duration::from_millis(500), event_stream.next()).await {
            println!("Received event: {:?}", evt.event_type());
            // We might receive the echoed JobQueueDepthChanged first
            if matches!(evt, DomainEvent::AutoScalingTriggered { .. }) {
                received_target = Some(evt);
                break;
            }
        }
    }

    match received_target {
        Some(evt) => {
            println!("Successfully received target event: {:?}", evt);
            assert_eq!(evt.event_type(), "AutoScalingTriggered");
        }
        None => {
            panic!("Did not receive AutoScalingTriggered event within timeout");
        }
    }
}
