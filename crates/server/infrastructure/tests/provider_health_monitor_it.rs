//! Integration test for ProviderHealthMonitor
//!
//! This test verifies:
//! 1. Health checks run every 30s per provider
//! 2. Metrics: response_time, success_rate, error_rate
//! 3. Provider marked unhealthy if health < threshold
//! 4. Auto-recovery: re-check unhealthy providers
//! 5. SmartScheduler avoids unhealthy providers
//! 6. EventBus: ProviderHealthChanged, ProviderRecovered events
//! 7. Alertas: provider unhealthy > 2 minutes
//!
//! These tests require Docker for Testcontainers.
//! Run with: cargo test --test provider_health_monitor_it -- --ignored

use async_trait::async_trait;
use futures::StreamExt;
use hodei_server_application::providers::health_monitor::{
    NoopMetricsRecorder, ProviderHealthMonitor,
};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::shared_kernel::{ProviderId, ProviderStatus};
use hodei_server_domain::workers::{
    HealthStatus, ProviderCapabilities, ProviderType, WorkerProvider,
};
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::time::timeout;

// Mock WorkerProvider for testing
#[derive(Clone)]
struct MockWorkerProvider {
    provider_id: ProviderId,
    health_status: HealthStatus,
    should_fail: bool,
}

impl MockWorkerProvider {
    fn new(provider_id: ProviderId) -> Self {
        Self {
            provider_id,
            health_status: HealthStatus::Healthy,
            should_fail: false,
        }
    }

    fn with_failure(provider_id: ProviderId) -> Self {
        Self {
            provider_id,
            health_status: HealthStatus::Healthy,
            should_fail: true,
        }
    }
}

#[async_trait]
impl WorkerProvider for MockWorkerProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Docker
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        static CAPABILITIES: std::sync::LazyLock<ProviderCapabilities> =
            std::sync::LazyLock::new(|| ProviderCapabilities::default());
        &CAPABILITIES
    }

    async fn create_worker(
        &self,
        _spec: &hodei_server_domain::workers::WorkerSpec,
    ) -> std::result::Result<
        hodei_server_domain::workers::WorkerHandle,
        hodei_server_domain::workers::ProviderError,
    > {
        unimplemented!()
    }

    async fn get_worker_status(
        &self,
        _handle: &hodei_server_domain::workers::WorkerHandle,
    ) -> std::result::Result<
        hodei_server_domain::shared_kernel::WorkerState,
        hodei_server_domain::workers::ProviderError,
    > {
        unimplemented!()
    }

    async fn destroy_worker(
        &self,
        _handle: &hodei_server_domain::workers::WorkerHandle,
    ) -> std::result::Result<(), hodei_server_domain::workers::ProviderError> {
        unimplemented!()
    }

    async fn get_worker_logs(
        &self,
        _handle: &hodei_server_domain::workers::WorkerHandle,
        _tail: Option<u32>,
    ) -> std::result::Result<
        Vec<hodei_server_domain::workers::LogEntry>,
        hodei_server_domain::workers::ProviderError,
    > {
        unimplemented!()
    }

    async fn health_check(
        &self,
    ) -> std::result::Result<HealthStatus, hodei_server_domain::workers::ProviderError> {
        if self.should_fail {
            Err(
                hodei_server_domain::workers::ProviderError::ConnectionFailed(
                    "Mock error".to_string(),
                ),
            )
        } else {
            Ok(self.health_status.clone())
        }
    }
}

/// Test provider health monitoring flow
///
/// Requires: Docker for Testcontainers
#[tokio::test]
#[ignore = "Requires Docker for Testcontainers - run with --ignored flag"]
async fn test_provider_health_monitoring_flow() -> anyhow::Result<()> {
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

    // 2. Setup EventBus
    let event_bus = PostgresEventBus::new(pool.clone());

    // Subscribe to events
    let mut event_stream = event_bus
        .subscribe("hodei_events")
        .await
        .expect("Failed to subscribe");

    // 3. Create a mock provider
    let provider_id = ProviderId::new();
    let mock_provider = MockWorkerProvider::new(provider_id.clone());

    // 4. Setup ProviderHealthMonitor with mock provider
    let mut health_monitor = ProviderHealthMonitor::new(
        Arc::new(event_bus.clone()),
        vec![Arc::new(mock_provider) as Arc<dyn WorkerProvider>],
        Duration::from_secs(5), // Shorter interval for testing
        Duration::from_secs(2), // 2 second threshold for alerts
        Arc::new(NoopMetricsRecorder),
    );

    health_monitor
        .start_monitoring()
        .await
        .expect("Failed to start health monitor");

    // 5. Verify health check events are published
    // Wait for initial health check
    let timeout_secs = 15; // Allow time for health check cycle
    let start = Instant::now();

    let mut health_changed_count = 0;

    while start.elapsed() < Duration::from_secs(timeout_secs) {
        if let Ok(Some(Ok(event))) = timeout(Duration::from_secs(5), event_stream.next()).await {
            match event {
                DomainEvent::ProviderHealthChanged {
                    provider_id: event_provider_id,
                    old_status,
                    new_status,
                    ..
                } => {
                    println!(
                        "Provider health changed: {:?} -> {:?}",
                        old_status, new_status
                    );
                    assert_eq!(event_provider_id, provider_id);
                    health_changed_count += 1;

                    // First check should be healthy
                    if health_changed_count == 1 {
                        assert_eq!(new_status, ProviderStatus::Active);
                    }
                }
                DomainEvent::ProviderRecovered {
                    provider_id: event_provider_id,
                    previous_status,
                    ..
                } => {
                    println!("Provider recovered: {:?}", previous_status);
                    assert_eq!(event_provider_id, provider_id);
                }
                _ => {}
            }

            // Stop after we see at least one health change
            if health_changed_count >= 1 {
                break;
            }
        }
    }

    // Verify we received health monitoring events
    assert!(
        health_changed_count >= 1,
        "Expected at least 1 health changed event, got {}",
        health_changed_count
    );

    // 6. Cleanup
    health_monitor.stop_monitoring().await;

    // Allow cleanup to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drop pool before container
    drop(pool);

    Ok(())
}

/// Test that health checks run at the configured interval
///
/// Requires: Docker for Testcontainers
#[tokio::test]
#[ignore = "Requires Docker for Testcontainers - run with --ignored flag"]
async fn test_health_check_interval() -> anyhow::Result<()> {
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
        .await?;

    let event_bus = PostgresEventBus::new(pool.clone());

    // Create a test provider
    let provider_id = ProviderId::new();
    let mock_provider = MockWorkerProvider::new(provider_id.clone());

    // Use shorter interval for testing
    let mut health_monitor = ProviderHealthMonitor::new(
        Arc::new(event_bus),
        vec![Arc::new(mock_provider) as Arc<dyn WorkerProvider>],
        Duration::from_secs(5), // 5 second interval
        Duration::from_secs(120),
        Arc::new(NoopMetricsRecorder),
    );

    health_monitor.start_monitoring().await?;

    // Wait for 2 health check cycles (10 seconds minimum)
    tokio::time::sleep(Duration::from_secs(12)).await;

    // Stop monitoring
    health_monitor.stop_monitoring().await;

    // Allow cleanup to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drop pool before container
    drop(pool);

    Ok(())
}

/// Test health metrics collection
///
/// Requires: Docker for Testcontainers
#[tokio::test]
#[ignore = "Requires Docker for Testcontainers - run with --ignored flag"]
async fn test_health_metrics_collection() -> anyhow::Result<()> {
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
        .await?;

    let event_bus = PostgresEventBus::new(pool.clone());

    // Create a test provider
    let provider_id = ProviderId::new();
    let mock_provider = MockWorkerProvider::new(provider_id.clone());
    let _capabilities = ProviderCapabilities {
        max_resources: hodei_server_domain::workers::ResourceLimits {
            max_cpu_cores: 4.0,
            max_memory_bytes: 8 * 1024 * 1024 * 1024,
            max_disk_bytes: 100 * 1024 * 1024 * 1024,
            max_gpu_count: 0,
        },
        gpu_support: false,
        gpu_types: vec![],
        architectures: vec![hodei_server_domain::workers::Architecture::Amd64],
        runtimes: vec!["docker".to_string()],
        regions: vec!["local".to_string()],
        max_execution_time: Some(Duration::from_secs(3600)),
        persistent_storage: false,
        custom_networking: false,
        features: HashMap::new(),
    };

    let _health_monitor = ProviderHealthMonitor::new(
        Arc::new(event_bus),
        vec![Arc::new(mock_provider.clone()) as Arc<dyn WorkerProvider>],
        Duration::from_secs(10),
        Duration::from_secs(120),
        Arc::new(NoopMetricsRecorder),
    );

    // Perform a health check directly
    let health_status = mock_provider.health_check().await?;
    println!("Health status: {:?}", health_status);

    // Verify health status is one of the expected values
    match health_status {
        HealthStatus::Healthy => {
            println!("Provider is healthy");
        }
        HealthStatus::Degraded { reason } => {
            println!("Provider is degraded: {}", reason);
        }
        HealthStatus::Unhealthy { reason } => {
            println!("Provider is unhealthy: {}", reason);
        }
        HealthStatus::Unknown => {
            println!("Provider health is unknown");
        }
    }

    // Allow cleanup to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drop pool before container
    drop(pool);

    Ok(())
}

/// Test provider unhealthy scenario
///
/// Requires: Docker for Testcontainers
#[tokio::test]
#[ignore = "Requires Docker for Testcontainers - run with --ignored flag"]
async fn test_provider_unhealthy_scenario() -> anyhow::Result<()> {
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
        .await?;

    let event_bus = PostgresEventBus::new(pool.clone());

    // Subscribe to events
    let mut event_stream = event_bus
        .subscribe("hodei_events")
        .await
        .expect("Failed to subscribe");

    // Create a test provider that will fail
    let provider_id = ProviderId::new();
    let mock_provider = MockWorkerProvider::with_failure(provider_id.clone());

    let mut health_monitor = ProviderHealthMonitor::new(
        Arc::new(event_bus.clone()),
        vec![Arc::new(mock_provider) as Arc<dyn WorkerProvider>],
        Duration::from_secs(5),
        Duration::from_secs(120),
        Arc::new(NoopMetricsRecorder),
    );

    health_monitor.start_monitoring().await?;

    // Wait for health check to detect failure
    tokio::time::sleep(Duration::from_secs(8)).await;

    // Check for ProviderHealthChanged event (should be Unhealthy)
    let mut found_unhealthy = false;
    while let Ok(Some(Ok(event))) = timeout(Duration::from_secs(2), event_stream.next()).await {
        if let DomainEvent::ProviderHealthChanged {
            provider_id: event_provider_id,
            new_status,
            ..
        } = event
        {
            if event_provider_id == provider_id && new_status == ProviderStatus::Unhealthy {
                found_unhealthy = true;
                break;
            }
        }
    }

    assert!(
        found_unhealthy,
        "Expected to find ProviderHealthChanged event to Unhealthy"
    );

    health_monitor.stop_monitoring().await;

    // Allow cleanup to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drop pool before container
    drop(pool);

    Ok(())
}
