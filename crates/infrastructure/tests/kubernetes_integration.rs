//! Kubernetes Provider Integration Tests
//!
//! These tests require a Kubernetes cluster (kind, minikube, or real cluster).
//! Run with: HODEI_K8S_TEST=1 cargo test --test kubernetes_integration
//!
//! Setup:
//! 1. Install kind: https://kind.sigs.k8s.io/
//! 2. Create cluster: kind create cluster --name hodei-test
//! 3. Apply manifests: kubectl apply -f deploy/kubernetes/
//! 4. Run tests: HODEI_K8S_TEST=1 cargo test --test kubernetes_integration

use hodei_jobs_domain::{
    shared_kernel::{WorkerId, WorkerState},
    worker::{ProviderType, WorkerSpec},
    worker_provider::{HealthStatus, WorkerProvider},
};
use hodei_jobs_infrastructure::providers::{KubernetesConfig, KubernetesProvider};
use std::time::Duration;

fn should_run_k8s_tests() -> bool {
    std::env::var("HODEI_K8S_TEST").unwrap_or_default() == "1"
}

fn get_test_config() -> KubernetesConfig {
    KubernetesConfig::builder()
        .namespace(
            std::env::var("HODEI_K8S_TEST_NAMESPACE")
                .unwrap_or_else(|_| "hodei-jobs-workers".to_string()),
        )
        .build()
        .expect("Failed to build test config")
}

#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_kubernetes_provider_health_check() {
    if !should_run_k8s_tests() {
        return;
    }

    let config = get_test_config();
    let provider = KubernetesProvider::with_config(config)
        .await
        .expect("Failed to create provider");

    let health = provider.health_check().await.expect("Health check failed");

    match health {
        HealthStatus::Healthy => {
            println!("✓ Kubernetes provider is healthy");
        }
        HealthStatus::Degraded { reason } => {
            println!("⚠ Kubernetes provider is degraded: {}", reason);
        }
        HealthStatus::Unhealthy { reason } => {
            panic!("✗ Kubernetes provider is unhealthy: {}", reason);
        }
        HealthStatus::Unknown => {
            println!("? Kubernetes provider status unknown");
        }
    }
}

#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_kubernetes_provider_create_and_destroy_worker() {
    if !should_run_k8s_tests() {
        return;
    }

    let config = get_test_config();
    let provider = KubernetesProvider::with_config(config)
        .await
        .expect("Failed to create provider");

    // Verify provider is healthy
    let health = provider.health_check().await.expect("Health check failed");
    assert!(
        matches!(health, HealthStatus::Healthy | HealthStatus::Degraded { .. }),
        "Provider must be healthy or degraded to run tests"
    );

    // Create worker spec
    let worker_id = WorkerId::new();
    let mut spec = WorkerSpec::new(
        "alpine:latest".to_string(),
        "http://localhost:50051".to_string(),
    )
    .with_env("TEST_VAR", "test_value");
    spec.worker_id = worker_id.clone();

    println!("Creating worker pod: {}", worker_id);

    // Create worker
    let handle = provider
        .create_worker(&spec)
        .await
        .expect("Failed to create worker");

    assert_eq!(handle.worker_id, worker_id);
    assert_eq!(handle.provider_type, ProviderType::Kubernetes);
    println!("✓ Worker pod created: {}", handle.provider_resource_id);

    // Wait a bit for pod to start
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check status
    let status = provider
        .get_worker_status(&handle)
        .await
        .expect("Failed to get worker status");

    println!("Worker status: {:?}", status);
    assert!(
        matches!(
            status,
            WorkerState::Creating | WorkerState::Connecting | WorkerState::Ready
        ),
        "Worker should be in Creating, Connecting, or Ready state"
    );

    // Get logs (may be empty if pod just started)
    let logs = provider
        .get_worker_logs(&handle, Some(10))
        .await
        .expect("Failed to get worker logs");

    println!("Worker logs: {} entries", logs.len());

    // Destroy worker
    provider
        .destroy_worker(&handle)
        .await
        .expect("Failed to destroy worker");

    println!("✓ Worker pod destroyed");

    // Verify worker is terminated
    tokio::time::sleep(Duration::from_secs(2)).await;
    let final_status = provider
        .get_worker_status(&handle)
        .await
        .expect("Failed to get final status");

    assert!(
        matches!(final_status, WorkerState::Terminated),
        "Worker should be terminated after destroy"
    );
    println!("✓ Worker confirmed terminated");
}

#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_kubernetes_provider_destroy_nonexistent_worker() {
    if !should_run_k8s_tests() {
        return;
    }

    let config = get_test_config();
    let provider = KubernetesProvider::with_config(config)
        .await
        .expect("Failed to create provider");

    // Create a handle for a non-existent worker
    let worker_id = WorkerId::new();
    let handle = hodei_jobs_domain::worker::WorkerHandle::new(
        worker_id,
        "hodei-jobs-worker-nonexistent".to_string(),
        ProviderType::Kubernetes,
        provider.provider_id().clone(),
    );

    // Destroy should be idempotent (not fail for non-existent)
    let result = provider.destroy_worker(&handle).await;
    assert!(result.is_ok(), "Destroy should be idempotent for non-existent pods");
    println!("✓ Destroy is idempotent for non-existent pods");
}

#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_kubernetes_provider_capabilities() {
    if !should_run_k8s_tests() {
        return;
    }

    let config = get_test_config();
    let provider = KubernetesProvider::with_config(config)
        .await
        .expect("Failed to create provider");

    let capabilities = provider.capabilities();

    assert!(capabilities.max_resources.max_cpu_cores > 0.0);
    assert!(capabilities.max_resources.max_memory_bytes > 0);
    assert!(!capabilities.architectures.is_empty());

    println!("✓ Provider capabilities:");
    println!("  - Max CPU: {} cores", capabilities.max_resources.max_cpu_cores);
    println!(
        "  - Max Memory: {} GB",
        capabilities.max_resources.max_memory_bytes / (1024 * 1024 * 1024)
    );
    println!("  - GPU Support: {}", capabilities.gpu_support);
    println!("  - Architectures: {:?}", capabilities.architectures);

    let startup_time = provider.estimated_startup_time();
    println!("  - Estimated startup time: {:?}", startup_time);
    assert!(startup_time.as_secs() > 0);
}

#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_kubernetes_provider_duplicate_worker_fails() {
    if !should_run_k8s_tests() {
        return;
    }

    let config = get_test_config();
    let provider = KubernetesProvider::with_config(config)
        .await
        .expect("Failed to create provider");

    // Create worker spec
    let worker_id = WorkerId::new();
    let mut spec = WorkerSpec::new(
        "alpine:latest".to_string(),
        "http://localhost:50051".to_string(),
    );
    spec.worker_id = worker_id.clone();

    // Create first worker
    let handle = provider
        .create_worker(&spec)
        .await
        .expect("Failed to create first worker");

    println!("✓ First worker created: {}", handle.provider_resource_id);

    // Try to create duplicate (same worker_id)
    let duplicate_result = provider.create_worker(&spec).await;
    assert!(
        duplicate_result.is_err(),
        "Creating duplicate worker should fail"
    );
    println!("✓ Duplicate worker creation correctly rejected");

    // Cleanup
    provider
        .destroy_worker(&handle)
        .await
        .expect("Failed to cleanup worker");
    println!("✓ Cleanup complete");
}
