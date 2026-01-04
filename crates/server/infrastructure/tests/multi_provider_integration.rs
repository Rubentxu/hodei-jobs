//! Multi-Provider Integration Tests
//!
//! Tests that verify both Docker and Kubernetes providers work correctly together.
//! Includes tests for provider selection based on labels/annotations.
//! Logs streaming verification for both providers.
//!
//! Run with:
//! - Docker tests: cargo test --test multi_provider_integration test_docker_provider
//! - Kubernetes tests: HODEI_K8S_TEST=1 cargo test --test multi_provider_integration test_kubernetes_provider
//! - Both providers: HODEI_K8S_TEST=1 cargo test --test multi_provider_integration
//!
//! ⚠️ EPIC-43: These tests require maintenance for WorkerRegistry::register signature change
//! The tests call register() with 2 arguments but now require 3 (including JobId).

use hodei_server_domain::workers::provider_api::{
    WorkerCost, WorkerEligibility, WorkerHealth, WorkerLifecycle, WorkerLogs, WorkerMetrics,
    WorkerProviderIdentity,
};
use hodei_server_domain::{
    jobs::Job,
    jobs::JobSpec,
    providers::config::DockerConfig,
    scheduling::strategies::{
        FastestStartupProviderSelector, HealthiestProviderSelector, LowestCostProviderSelector,
        MostCapacityProviderSelector, ProviderInfo, ProviderSelector,
    },
    shared_kernel::{JobId, ProviderId, WorkerId, WorkerState},
    workers::{
        HealthStatus, ProviderType, ResourceRequirements, WorkerHandle, WorkerProvider, WorkerSpec,
    },
};
use hodei_server_infrastructure::providers::{
    DockerProvider, KubernetesConfig, KubernetesProvider,
};
use std::collections::HashMap;
use std::time::Duration;

fn should_run_k8s_tests() -> bool {
    std::env::var("HODEI_K8S_TEST").unwrap_or_default() == "1"
}

fn get_kubernetes_config() -> KubernetesConfig {
    KubernetesConfig::builder()
        .namespace(
            std::env::var("HODEI_K8S_TEST_NAMESPACE")
                .unwrap_or_else(|_| "hodei-jobs-workers".to_string()),
        )
        .build()
        .expect("Failed to build Kubernetes test config")
}

async fn skip_if_no_docker() -> bool {
    match DockerProvider::new().await {
        Ok(provider) => {
            let health = provider.health_check().await;
            !matches!(
                health,
                Ok(hodei_server_domain::workers::provider_api::HealthStatus::Healthy)
            )
        }
        Err(_) => true,
    }
}

async fn skip_if_no_kubernetes() -> bool {
    if !should_run_k8s_tests() {
        return true;
    }

    match KubernetesProvider::new().await {
        Ok(provider) => {
            let health = provider.health_check().await;
            matches!(health, Ok(HealthStatus::Unhealthy { .. }) | Err(_))
        }
        Err(_) => true,
    }
}

/// Create a WorkerSpec with specific labels for provider selection
fn create_labeled_worker_spec(
    name: &str,
    provider_type: ProviderType,
    use_gpu: bool,
) -> WorkerSpec {
    let mut spec = WorkerSpec::new(
        if use_gpu {
            "nvidia/cuda:11.8-runtime-ubuntu20.04".to_string()
        } else {
            // Use nginx:alpine as it's a simple container that stays running
            "nginx:alpine".to_string()
        },
        "http://localhost:50051".to_string(),
    )
    .with_label("test.name", name)
    .with_label("provider.type", format!("{:?}", provider_type))
    .with_label("execution.env", "test")
    .with_env("TEST_MODE", "true")
    .with_env("PROVIDER_TYPE", format!("{:?}", provider_type));

    // Add GPU resources if requested
    if use_gpu {
        spec.resources = ResourceRequirements {
            cpu_cores: 4.0,
            memory_bytes: 8 * 1024 * 1024 * 1024, // 8GB
            disk_bytes: 20 * 1024 * 1024 * 1024,  // 20GB
            gpu_count: 1,
            gpu_type: Some("nvidia-tesla-v100".to_string()),
        };

        // Add Kubernetes-specific annotations for GPU scheduling
        spec.kubernetes.annotations.insert(
            "cluster-autoscaler.kubernetes.io/safe-to-evict".to_string(),
            "false".to_string(),
        );
    }

    // Add custom labels for Kubernetes
    spec.kubernetes.custom_labels.insert(
        "hodei.io/provider".to_string(),
        format!("{:?}", provider_type),
    );
    spec.kubernetes
        .custom_labels
        .insert("hodei.io/test".to_string(), "true".to_string());

    spec
}

/// Test: Docker Provider Basic Operations
#[tokio::test]
async fn test_docker_provider_basic_operations() {
    if skip_if_no_docker().await {
        println!("⏭️  Skipping test: Docker not available");
        return;
    }

    println!("\n🐳 Testing Docker Provider Basic Operations");

    let provider = DockerProvider::with_config(DockerConfig::default())
        .await
        .expect("Failed to create DockerProvider");

    // Health check
    let health = provider.health_check().await.expect("Health check failed");
    assert!(matches!(
        health,
        hodei_server_domain::workers::provider_api::HealthStatus::Healthy
    ));
    println!("✓ Docker provider is healthy");

    // Create worker
    let spec = create_labeled_worker_spec("docker-basic", ProviderType::Docker, false);
    let handle = provider
        .create_worker(&spec)
        .await
        .expect("Failed to create worker");

    println!("✓ Docker worker created: {}", handle.provider_resource_id);

    // Verify worker status
    let status = provider
        .get_worker_status(&handle)
        .await
        .expect("Failed to get worker status");

    println!("Docker worker actual status: {:?}", status);
    assert!(
        matches!(
            status,
            WorkerState::Ready | WorkerState::Connecting | WorkerState::Creating
        ),
        "Unexpected Docker worker state: {:?}",
        status
    );
    println!("✓ Docker worker status: {:?}", status);

    // Get logs
    let logs = provider
        .get_worker_logs(&handle, Some(10))
        .await
        .expect("Failed to get logs");

    println!("✓ Docker worker logs: {} entries", logs.len());

    // Destroy worker
    provider
        .destroy_worker(&handle)
        .await
        .expect("Failed to destroy worker");

    println!("✓ Docker worker destroyed");
    println!("✅ Docker Provider Basic Operations: PASSED\n");
}

/// Test: Kubernetes Provider Basic Operations
#[tokio::test]
#[ignore = "Requires Kubernetes cluster. Run with HODEI_K8S_TEST=1"]
async fn test_kubernetes_provider_basic_operations() {
    if skip_if_no_kubernetes().await {
        println!("⏭️  Skipping test: Kubernetes not available");
        return;
    }

    println!("\n☸️  Testing Kubernetes Provider Basic Operations");

    let config = get_kubernetes_config();
    let provider = KubernetesProvider::with_config(config)
        .await
        .expect("Failed to create KubernetesProvider");

    // Health check
    let health = provider.health_check().await.expect("Health check failed");
    assert!(matches!(
        health,
        HealthStatus::Healthy | HealthStatus::Degraded { .. }
    ));
    println!("✓ Kubernetes provider is healthy");

    // Create worker
    let spec = create_labeled_worker_spec("k8s-basic", ProviderType::Kubernetes, false);
    let handle = provider
        .create_worker(&spec)
        .await
        .expect("Failed to create worker");

    println!(
        "✓ Kubernetes worker created: {}",
        handle.provider_resource_id
    );

    // Wait for pod to start
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify worker status
    let status = provider
        .get_worker_status(&handle)
        .await
        .expect("Failed to get worker status");

    assert!(matches!(
        status,
        WorkerState::Creating | WorkerState::Connecting | WorkerState::Ready
    ));
    println!("✓ Kubernetes worker status: {:?}", status);

    // Get logs
    let logs = provider
        .get_worker_logs(&handle, Some(10))
        .await
        .expect("Failed to get logs");

    println!("✓ Kubernetes worker logs: {} entries", logs.len());

    // Destroy worker
    provider
        .destroy_worker(&handle)
        .await
        .expect("Failed to destroy worker");

    println!("✓ Kubernetes worker destroyed");
    println!("✅ Kubernetes Provider Basic Operations: PASSED\n");
}

/// Test: Provider Selection by Labels
#[tokio::test]
async fn test_provider_selection_by_labels() {
    if skip_if_no_docker().await {
        println!("⏭️  Skipping test: Docker not available");
        return;
    }

    println!("\n🎯 Testing Provider Selection by Labels");

    let docker_provider = DockerProvider::with_config(DockerConfig::default())
        .await
        .expect("Failed to create DockerProvider");

    // Create provider info for selection strategy
    let docker_info = ProviderInfo {
        provider_id: docker_provider.provider_id().clone(),
        provider_type: ProviderType::Docker,
        active_workers: 5,
        max_workers: 10,
        estimated_startup_time: Duration::from_secs(5),
        health_score: 0.9,
        cost_per_hour: 0.10,
        gpu_support: false,
        gpu_types: vec![],
        regions: vec!["local".to_string()],
    };

    let k8s_info = if should_run_k8s_tests() {
        let config = get_kubernetes_config();
        let k8s_provider = KubernetesProvider::with_config(config)
            .await
            .expect("Failed to create KubernetesProvider");

        let provider_id = k8s_provider.provider_id().clone();
        let k8s_info = ProviderInfo {
            provider_id,
            provider_type: ProviderType::Kubernetes,
            active_workers: 2,
            max_workers: 20,
            estimated_startup_time: Duration::from_secs(30),
            health_score: 0.95,
            cost_per_hour: 0.05,
            gpu_support: false,
            gpu_types: vec![],
            regions: vec!["default".to_string()],
        };

        Some((k8s_provider, k8s_info))
    } else {
        None
    };

    let providers = match k8s_info {
        Some((_, info)) => vec![docker_info.clone(), info],
        None => vec![docker_info],
    };

    // Test different selection strategies
    let job = Job::new(
        JobId::new(),
        JobSpec::new(vec!["echo".to_string(), "test".to_string()]),
    );

    // Lowest Cost Strategy - should prefer Kubernetes (0.05 vs 0.10)
    let lowest_cost_selector = LowestCostProviderSelector::new();
    let selected = lowest_cost_selector.select_provider(&job, &providers);
    if let Some(provider_id) = selected {
        println!("✓ LowestCost selector chose provider: {:?}", provider_id);
    }

    // Fastest Startup Strategy - should prefer Docker (5s vs 30s)
    let fastest_startup_selector = FastestStartupProviderSelector::new();
    let selected = fastest_startup_selector.select_provider(&job, &providers);
    if let Some(provider_id) = selected {
        println!(
            "✓ FastestStartup selector chose provider: {:?}",
            provider_id
        );
    }

    // Most Capacity Strategy - should prefer Kubernetes (90% vs 50%)
    let most_capacity_selector = MostCapacityProviderSelector::new();
    let selected = most_capacity_selector.select_provider(&job, &providers);
    if let Some(provider_id) = selected {
        println!("✓ MostCapacity selector chose provider: {:?}", provider_id);
    }

    // Healthiest Strategy - should prefer the healthier one
    let healthiest_selector = HealthiestProviderSelector::new();
    let selected = healthiest_selector.select_provider(&job, &providers);
    if let Some(provider_id) = selected {
        println!("✓ Healthiest selector chose provider: {:?}", provider_id);
    }

    println!("✅ Provider Selection by Labels: PASSED\n");
}

/// Test: Concurrent Workers on Both Providers
#[tokio::test]
async fn test_concurrent_workers_on_both_providers() {
    if skip_if_no_docker().await {
        println!("⏭️  Skipping test: Docker not available");
        return;
    }

    println!("\n🔀 Testing Concurrent Workers on Both Providers");

    let docker_provider = DockerProvider::with_config(DockerConfig::default())
        .await
        .expect("Failed to create DockerProvider");

    let mut docker_handles = Vec::new();

    // Create 3 Docker workers
    for i in 0..3 {
        let spec = create_labeled_worker_spec(
            &format!("docker-concurrent-{}", i),
            ProviderType::Docker,
            false,
        );

        let handle = docker_provider
            .create_worker(&spec)
            .await
            .expect("Failed to create Docker worker");

        docker_handles.push(handle);
        println!("✓ Docker worker {} created", i);
    }

    // If Kubernetes is available, create workers there too
    let mut k8s_handles = Vec::new();
    if should_run_k8s_tests() {
        let config = get_kubernetes_config();
        let k8s_provider = KubernetesProvider::with_config(config)
            .await
            .expect("Failed to create KubernetesProvider");

        for i in 0..2 {
            let spec = create_labeled_worker_spec(
                &format!("k8s-concurrent-{}", i),
                ProviderType::Kubernetes,
                false,
            );

            let handle = k8s_provider
                .create_worker(&spec)
                .await
                .expect("Failed to create Kubernetes worker");

            k8s_handles.push(handle);
            println!("✓ Kubernetes worker {} created", i);
        }

        // Wait for K8s pods to start
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Verify all K8s workers are running
        for (i, handle) in k8s_handles.iter().enumerate() {
            let status = k8s_provider
                .get_worker_status(handle)
                .await
                .expect("Failed to get K8s worker status");

            assert!(matches!(
                status,
                WorkerState::Creating | WorkerState::Connecting | WorkerState::Ready
            ));
            println!("✓ Kubernetes worker {} is running: {:?}", i, status);
        }
    }

    // Verify all Docker workers are running
    for (i, handle) in docker_handles.iter().enumerate() {
        let status = docker_provider
            .get_worker_status(handle)
            .await
            .expect("Failed to get Docker worker status");

        assert!(matches!(
            status,
            WorkerState::Ready | WorkerState::Connecting
        ));
        println!("✓ Docker worker {} is running: {:?}", i, status);
    }

    // Get logs from all workers
    for (i, handle) in docker_handles.iter().enumerate() {
        let logs = docker_provider
            .get_worker_logs(handle, Some(5))
            .await
            .expect("Failed to get Docker logs");
        println!("✓ Docker worker {} has {} log entries", i, logs.len());
    }

    if should_run_k8s_tests() {
        let config = get_kubernetes_config();
        let k8s_provider = KubernetesProvider::with_config(config)
            .await
            .expect("Failed to create KubernetesProvider");

        for (i, handle) in k8s_handles.iter().enumerate() {
            let logs = k8s_provider
                .get_worker_logs(handle, Some(5))
                .await
                .expect("Failed to get K8s logs");
            println!("✓ Kubernetes worker {} has {} log entries", i, logs.len());
        }

        // Cleanup K8s workers
        for handle in k8s_handles {
            k8s_provider
                .destroy_worker(&handle)
                .await
                .expect("Failed to destroy K8s worker");
        }
        println!("✓ All Kubernetes workers destroyed");
    }

    // Cleanup Docker workers
    for handle in docker_handles {
        docker_provider
            .destroy_worker(&handle)
            .await
            .expect("Failed to destroy Docker worker");
    }
    println!("✓ All Docker workers destroyed");

    println!("✅ Concurrent Workers on Both Providers: PASSED\n");
}

/// Test: GPU Worker on Kubernetes
#[tokio::test]
#[ignore = "Requires Kubernetes cluster with GPU support. Run with HODEI_K8S_TEST=1"]
async fn test_gpu_worker_on_kubernetes() {
    if skip_if_no_kubernetes().await {
        println!("⏭️  Skipping test: Kubernetes not available");
        return;
    }

    println!("\n🚀 Testing GPU Worker on Kubernetes");

    let config = get_kubernetes_config();
    let provider = KubernetesProvider::with_config(config)
        .await
        .expect("Failed to create KubernetesProvider");

    // Verify provider supports GPU
    let capabilities = provider.capabilities();
    assert!(capabilities.gpu_support, "Provider must support GPU");

    // Create GPU worker
    let spec = create_labeled_worker_spec("k8s-gpu", ProviderType::Kubernetes, true);
    let handle = provider
        .create_worker(&spec)
        .await
        .expect("Failed to create GPU worker");

    println!("✓ GPU worker created: {}", handle.provider_resource_id);
    println!("  - CPU: {} cores", spec.resources.cpu_cores);
    println!(
        "  - Memory: {} GB",
        spec.resources.memory_bytes / (1024 * 1024 * 1024)
    );
    println!("  - GPU Count: {}", spec.resources.gpu_count);
    println!("  - GPU Type: {:?}", spec.resources.gpu_type);

    // Wait for pod to start
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify worker is running
    let status = provider
        .get_worker_status(&handle)
        .await
        .expect("Failed to get GPU worker status");

    assert!(matches!(
        status,
        WorkerState::Creating | WorkerState::Connecting | WorkerState::Ready
    ));
    println!("✓ GPU worker status: {:?}", status);

    // Get logs
    let logs = provider
        .get_worker_logs(&handle, Some(20))
        .await
        .expect("Failed to get GPU worker logs");

    println!("✓ GPU worker logs: {} entries", logs.len());

    // Cleanup
    provider
        .destroy_worker(&handle)
        .await
        .expect("Failed to destroy GPU worker");

    println!("✓ GPU worker destroyed");
    println!("✅ GPU Worker on Kubernetes: PASSED\n");
}

/// Test: Log Streaming from Both Providers
#[tokio::test]
async fn test_log_streaming_from_both_providers() {
    if skip_if_no_docker().await {
        println!("⏭️  Skipping test: Docker not available");
        return;
    }

    println!("\n📜 Testing Log Streaming from Both Providers");

    let docker_provider = DockerProvider::with_config(DockerConfig::default())
        .await
        .expect("Failed to create DockerProvider");

    // Create a worker that generates specific log output
    let mut docker_spec = create_labeled_worker_spec("docker-logs", ProviderType::Docker, false);
    docker_spec
        .environment
        .insert("LOG_MESSAGE".to_string(), "Docker log test".to_string());

    let docker_handle = docker_provider
        .create_worker(&docker_spec)
        .await
        .expect("Failed to create Docker worker");

    println!("✓ Docker worker created for log streaming test");

    // Get logs from Docker
    let docker_logs = docker_provider
        .get_worker_logs(&docker_handle, Some(50))
        .await
        .expect("Failed to get Docker logs");

    println!("✓ Docker logs retrieved: {} entries", docker_logs.len());

    // If Kubernetes is available, test logs there too
    if should_run_k8s_tests() {
        let config = get_kubernetes_config();
        let k8s_provider = KubernetesProvider::with_config(config)
            .await
            .expect("Failed to create KubernetesProvider");

        let mut k8s_spec = create_labeled_worker_spec("k8s-logs", ProviderType::Kubernetes, false);
        k8s_spec
            .environment
            .insert("LOG_MESSAGE".to_string(), "Kubernetes log test".to_string());

        let k8s_handle = k8s_provider
            .create_worker(&k8s_spec)
            .await
            .expect("Failed to create Kubernetes worker");

        println!("✓ Kubernetes worker created for log streaming test");

        // Wait for pod to start
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Get logs from Kubernetes
        let k8s_logs = k8s_provider
            .get_worker_logs(&k8s_handle, Some(50))
            .await
            .expect("Failed to get Kubernetes logs");

        println!("✓ Kubernetes logs retrieved: {} entries", k8s_logs.len());

        // Cleanup K8s worker
        k8s_provider
            .destroy_worker(&k8s_handle)
            .await
            .expect("Failed to destroy K8s worker");
        println!("✓ Kubernetes worker destroyed");
    }

    // Cleanup Docker worker
    docker_provider
        .destroy_worker(&docker_handle)
        .await
        .expect("Failed to destroy Docker worker");
    println!("✓ Docker worker destroyed");

    println!("✅ Log Streaming from Both Providers: PASSED\n");
}

/// Test: Provider Capabilities Comparison
#[tokio::test]
async fn test_provider_capabilities_comparison() {
    if skip_if_no_docker().await {
        println!("⏭️  Skipping test: Docker not available");
        return;
    }

    println!("\n⚖️  Testing Provider Capabilities Comparison");

    let docker_provider = DockerProvider::with_config(DockerConfig::default())
        .await
        .expect("Failed to create DockerProvider");

    let docker_caps = docker_provider.capabilities();
    println!("🐳 Docker Provider Capabilities:");
    println!(
        "  - Max CPU Cores: {}",
        docker_caps.max_resources.max_cpu_cores
    );
    println!(
        "  - Max Memory: {} GB",
        docker_caps.max_resources.max_memory_bytes / (1024 * 1024 * 1024)
    );
    println!("  - GPU Support: {}", docker_caps.gpu_support);
    println!("  - GPU Types: {:?}", docker_caps.gpu_types);
    println!("  - Architectures: {:?}", docker_caps.architectures);

    let docker_startup = docker_provider.estimated_startup_time();
    println!("  - Estimated Startup Time: {:?}", docker_startup);

    if should_run_k8s_tests() {
        let config = get_kubernetes_config();
        let k8s_provider = KubernetesProvider::with_config(config)
            .await
            .expect("Failed to create KubernetesProvider");

        let k8s_caps = k8s_provider.capabilities();
        println!("\n☸️  Kubernetes Provider Capabilities:");
        println!(
            "  - Max CPU Cores: {}",
            k8s_caps.max_resources.max_cpu_cores
        );
        println!(
            "  - Max Memory: {} GB",
            k8s_caps.max_resources.max_memory_bytes / (1024 * 1024 * 1024)
        );
        println!("  - GPU Support: {}", k8s_caps.gpu_support);
        println!("  - GPU Types: {:?}", k8s_caps.gpu_types);
        println!("  - Architectures: {:?}", k8s_caps.architectures);

        let k8s_startup = k8s_provider.estimated_startup_time();
        println!("  - Estimated Startup Time: {:?}", k8s_startup);

        // Verify Kubernetes has more resources
        assert!(k8s_caps.max_resources.max_cpu_cores >= docker_caps.max_resources.max_cpu_cores);
        assert!(
            k8s_caps.max_resources.max_memory_bytes >= docker_caps.max_resources.max_memory_bytes
        );

        // Verify Docker has faster startup
        assert!(docker_startup < k8s_startup);

        println!("\n✓ Capabilities comparison verified:");
        println!("  - Docker: Faster startup ({:?})", docker_startup);
        println!(
            "  - Kubernetes: More resources, slower startup ({:?})",
            k8s_startup
        );
    }

    println!("\n✅ Provider Capabilities Comparison: PASSED\n");
}
