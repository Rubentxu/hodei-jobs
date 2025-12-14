//! Integration tests for DockerProvider
//!
//! Uses real Docker daemon for testing. Requires Docker to be running.
//! Pattern: Single Instance + Resource Pooling for TestContainers optimization.

use hodei_jobs_domain::{
    provider_config::DockerConfig,
    shared_kernel::WorkerState,
    worker::{ProviderType, WorkerSpec},
    worker_provider::WorkerProvider,
};
use hodei_jobs_infrastructure::providers::DockerProvider;
use tokio::sync::OnceCell;

/// Global Docker provider instance for test reuse (Single Instance pattern)
static DOCKER_PROVIDER: OnceCell<DockerProvider> = OnceCell::const_new();

/// Get or create the shared DockerProvider instance
async fn get_docker_provider() -> &'static DockerProvider {
    DOCKER_PROVIDER
        .get_or_init(|| async {
            DockerProvider::with_config(DockerConfig::default())
                .await
                .expect("Failed to create DockerProvider")
        })
        .await
}

/// Skip test if Docker is not available
async fn skip_if_no_docker() -> bool {
    match DockerProvider::new().await {
        Ok(provider) => {
            let health = provider.health_check().await;
            !matches!(health, Ok(hodei_jobs_domain::worker_provider::HealthStatus::Healthy))
        }
        Err(_) => true,
    }
}

/// Create a test WorkerSpec
fn create_test_worker_spec(name: &str) -> WorkerSpec {
    WorkerSpec::new(
        "alpine:latest".to_string(),
        "http://localhost:8080".to_string(),
    )
    .with_label("test.name", name)
    .with_env("TEST_MODE", "true")
}

#[tokio::test]
async fn test_docker_provider_health_check() {
    if skip_if_no_docker().await {
        println!("Skipping test: Docker not available");
        return;
    }

    let provider = get_docker_provider().await;
    let health = provider.health_check().await;

    assert!(health.is_ok());
    let status = health.unwrap();
    assert!(matches!(status, hodei_jobs_domain::worker_provider::HealthStatus::Healthy));
}

#[tokio::test]
async fn test_docker_provider_capabilities() {
    if skip_if_no_docker().await {
        println!("Skipping test: Docker not available");
        return;
    }

    let provider = get_docker_provider().await;
    let caps = provider.capabilities();

    assert!(caps.max_resources.max_cpu_cores > 0.0);
    assert!(caps.max_resources.max_memory_bytes > 0);
    assert!(!caps.architectures.is_empty());
}

#[tokio::test]
async fn test_docker_provider_type() {
    if skip_if_no_docker().await {
        println!("Skipping test: Docker not available");
        return;
    }

    let provider = get_docker_provider().await;
    assert_eq!(provider.provider_type(), ProviderType::Docker);
}

#[tokio::test]
async fn test_docker_provider_create_and_destroy_worker() {
    if skip_if_no_docker().await {
        println!("Skipping test: Docker not available");
        return;
    }

    let provider = get_docker_provider().await;
    let spec = create_test_worker_spec("create_destroy_test");

    // Create worker
    let handle = provider.create_worker(&spec).await;
    assert!(handle.is_ok(), "Failed to create worker: {:?}", handle.err());

    let handle = handle.unwrap();
    assert_eq!(handle.provider_type, ProviderType::Docker);
    assert!(!handle.provider_resource_id.is_empty());

    // Get status
    let status = provider.get_worker_status(&handle).await;
    assert!(status.is_ok());
    let state = status.unwrap();
    assert!(
        matches!(state, WorkerState::Ready | WorkerState::Connecting),
        "Unexpected state: {:?}",
        state
    );

    // Destroy worker
    let destroy_result = provider.destroy_worker(&handle).await;
    assert!(destroy_result.is_ok(), "Failed to destroy worker: {:?}", destroy_result.err());
}

#[tokio::test]
async fn test_docker_provider_get_worker_logs() {
    if skip_if_no_docker().await {
        println!("Skipping test: Docker not available");
        return;
    }

    let provider = get_docker_provider().await;
    
    // Use echo command to generate some logs
    let mut spec = create_test_worker_spec("logs_test");
    spec.image = "alpine:latest".to_string();

    let handle = provider.create_worker(&spec).await;
    assert!(handle.is_ok());
    let handle = handle.unwrap();

    // Wait a moment for container to start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Get logs
    let logs = provider.get_worker_logs(&handle, Some(10)).await;
    assert!(logs.is_ok());

    // Cleanup
    let _ = provider.destroy_worker(&handle).await;
}

#[tokio::test]
async fn test_docker_provider_worker_lifecycle() {
    if skip_if_no_docker().await {
        println!("Skipping test: Docker not available");
        return;
    }

    let provider = get_docker_provider().await;
    let spec = create_test_worker_spec("lifecycle_test");

    // 1. Create
    let handle = provider.create_worker(&spec).await.expect("Create failed");

    // 2. Verify running (PRD v6.0: Ready o Connecting)
    let state = provider.get_worker_status(&handle).await.expect("Status failed");
    assert!(
        matches!(state, WorkerState::Ready | WorkerState::Connecting),
        "Worker not running: {:?}",
        state
    );

    // 3. Get logs
    let _logs = provider.get_worker_logs(&handle, Some(5)).await.expect("Logs failed");

    // 4. Destroy
    provider.destroy_worker(&handle).await.expect("Destroy failed");

    // 5. Verify destroyed
    let status_after = provider.get_worker_status(&handle).await;
    assert!(status_after.is_err(), "Worker should not exist after destroy");
}
