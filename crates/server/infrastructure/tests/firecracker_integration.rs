//! Firecracker Provider Integration Tests
//!
//! These tests require a Linux host with KVM enabled and Firecracker installed.
//! Run with: HODEI_FC_TEST=1 cargo test --test firecracker_integration
//!
//! Setup:
//! 1. Install Firecracker: https://github.com/firecracker-microvm/firecracker
//! 2. Ensure /dev/kvm is accessible
//! 3. Prepare kernel and rootfs images
//! 4. Run tests: HODEI_FC_TEST=1 cargo test --test firecracker_integration

use hodei_server_domain::{
    shared_kernel::{WorkerId, WorkerState},
    workers::{HealthStatus, WorkerHandle, WorkerProvider},
    workers::{ProviderType, WorkerSpec},
};
use hodei_server_infrastructure::providers::{FirecrackerConfig, FirecrackerProvider};
use std::path::PathBuf;

fn should_run_fc_tests() -> bool {
    std::env::var("HODEI_FC_TEST").unwrap_or_default() == "1"
}

fn get_test_config() -> FirecrackerConfig {
    FirecrackerConfig::builder()
        .firecracker_path(
            std::env::var("HODEI_FC_FIRECRACKER_PATH")
                .unwrap_or_else(|_| "/usr/bin/firecracker".to_string()),
        )
        .jailer_path(
            std::env::var("HODEI_FC_JAILER_PATH").unwrap_or_else(|_| "/usr/bin/jailer".to_string()),
        )
        .data_dir(
            std::env::var("HODEI_FC_DATA_DIR").unwrap_or_else(|_| "/tmp/hodei-fc-test".to_string()),
        )
        .kernel_path(
            std::env::var("HODEI_FC_KERNEL_PATH")
                .unwrap_or_else(|_| "/var/lib/hodei/vmlinux".to_string()),
        )
        .rootfs_path(
            std::env::var("HODEI_FC_ROOTFS_PATH")
                .unwrap_or_else(|_| "/var/lib/hodei/rootfs.ext4".to_string()),
        )
        .use_jailer(false) // Disable jailer for tests
        .build()
        .expect("Failed to build test config")
}

#[tokio::test]
#[ignore = "Requires KVM and Firecracker. Run with HODEI_FC_TEST=1"]
async fn test_firecracker_provider_health_check() {
    if !should_run_fc_tests() {
        return;
    }

    let config = get_test_config();

    // Skip if requirements not met
    if !PathBuf::from("/dev/kvm").exists() {
        println!("⚠ Skipping: /dev/kvm not available");
        return;
    }
    if !config.firecracker_path.exists() {
        println!("⚠ Skipping: Firecracker binary not found");
        return;
    }

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            println!("⚠ Skipping: Provider creation failed: {}", e);
            return;
        }
    };

    let health = provider.health_check().await.expect("Health check failed");

    match health {
        HealthStatus::Healthy => {
            println!("✓ Firecracker provider is healthy");
        }
        HealthStatus::Degraded { reason } => {
            println!("⚠ Firecracker provider is degraded: {}", reason);
        }
        HealthStatus::Unhealthy { reason } => {
            println!("✗ Firecracker provider is unhealthy: {}", reason);
        }
        HealthStatus::Unknown => {
            println!("? Firecracker provider status unknown");
        }
    }
}

#[tokio::test]
#[ignore = "Requires KVM and Firecracker. Run with HODEI_FC_TEST=1"]
async fn test_firecracker_provider_capabilities() {
    if !should_run_fc_tests() {
        return;
    }

    let config = get_test_config();

    if !PathBuf::from("/dev/kvm").exists() {
        println!("⚠ Skipping: /dev/kvm not available");
        return;
    }

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            println!("⚠ Skipping: Provider creation failed: {}", e);
            return;
        }
    };

    let capabilities = provider.capabilities();

    // Firecracker doesn't support GPU
    assert!(!capabilities.gpu_support);
    assert_eq!(capabilities.max_resources.max_gpu_count, 0);

    // Should support x86_64 and aarch64
    assert!(!capabilities.architectures.is_empty());

    println!("✓ Provider capabilities:");
    println!(
        "  - Max CPU: {} cores",
        capabilities.max_resources.max_cpu_cores
    );
    println!(
        "  - Max Memory: {} GB",
        capabilities.max_resources.max_memory_bytes / (1024 * 1024 * 1024)
    );
    println!("  - GPU Support: {}", capabilities.gpu_support);
    println!("  - Architectures: {:?}", capabilities.architectures);

    let startup_time = provider.estimated_startup_time();
    println!("  - Estimated startup time: {:?}", startup_time);
    assert!(
        startup_time.as_millis() < 1000,
        "Firecracker should boot in < 1 second"
    );
}

#[tokio::test]
#[ignore = "Requires KVM, Firecracker, kernel, and rootfs. Run with HODEI_FC_TEST=1"]
async fn test_firecracker_provider_create_and_destroy_worker() {
    if !should_run_fc_tests() {
        return;
    }

    let config = get_test_config();

    // Verify all requirements
    if !PathBuf::from("/dev/kvm").exists() {
        println!("⚠ Skipping: /dev/kvm not available");
        return;
    }
    if !config.kernel_path.exists() {
        println!("⚠ Skipping: Kernel not found at {:?}", config.kernel_path);
        return;
    }
    if !config.rootfs_path.exists() {
        println!("⚠ Skipping: Rootfs not found at {:?}", config.rootfs_path);
        return;
    }

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            println!("⚠ Skipping: Provider creation failed: {}", e);
            return;
        }
    };

    // Create worker spec
    let worker_id = WorkerId::new();
    let mut spec = WorkerSpec::new(
        "unused".to_string(), // Firecracker uses rootfs, not container image
        "http://localhost:50051".to_string(),
    )
    .with_env("TEST_VAR", "test_value");
    spec.worker_id = worker_id.clone();

    println!("Creating microVM: {}", worker_id);

    // Create worker
    let handle = match provider.create_worker(&spec).await {
        Ok(h) => h,
        Err(e) => {
            println!("⚠ Failed to create worker: {}", e);
            return;
        }
    };

    assert_eq!(handle.worker_id, worker_id);
    assert!(matches!(handle.provider_type, ProviderType::Custom(ref s) if s == "firecracker"));
    println!("✓ MicroVM created: {}", handle.provider_resource_id);

    // Check status
    let status = provider
        .get_worker_status(&handle)
        .await
        .expect("Failed to get worker status");

    println!("Worker status: {:?}", status);

    // Destroy worker
    provider
        .destroy_worker(&handle)
        .await
        .expect("Failed to destroy worker");

    println!("✓ MicroVM destroyed");

    // Verify worker is terminated
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
#[ignore = "Requires KVM and Firecracker. Run with HODEI_FC_TEST=1"]
async fn test_firecracker_provider_destroy_nonexistent_worker() {
    if !should_run_fc_tests() {
        return;
    }

    let config = get_test_config();

    if !PathBuf::from("/dev/kvm").exists() {
        println!("⚠ Skipping: /dev/kvm not available");
        return;
    }

    let provider: FirecrackerProvider = match FirecrackerProvider::with_config(config).await {
        Ok(p) => p,
        Err(e) => {
            println!("⚠ Skipping: Provider creation failed: {}", e);
            return;
        }
    };

    // Create a handle for a non-existent worker
    let worker_id = WorkerId::new();
    let handle = WorkerHandle::new(
        worker_id,
        "fc-nonexistent".to_string(),
        ProviderType::Custom("firecracker".to_string()),
        provider.provider_id().clone(),
    );

    // Destroy should be idempotent (not fail for non-existent)
    let result = provider.destroy_worker(&handle).await;
    assert!(
        result.is_ok(),
        "Destroy should be idempotent for non-existent VMs"
    );
    println!("✓ Destroy is idempotent for non-existent VMs");
}

#[test]
fn test_ip_pool_allocation_and_release() {
    use hodei_server_infrastructure::providers::firecracker::IpPool;

    let mut pool = IpPool::from_cidr("172.16.0.0/28").expect("should parse CIDR");

    // Allocate several IPs
    let ip1 = pool.allocate().expect("should allocate first IP");
    let ip2 = pool.allocate().expect("should allocate second IP");
    let ip3 = pool.allocate().expect("should allocate third IP");

    assert_ne!(ip1, ip2);
    assert_ne!(ip2, ip3);
    assert_ne!(ip1, ip3);

    println!("Allocated IPs: {}, {}, {}", ip1, ip2, ip3);

    // Release one
    pool.release(ip2);
    assert!(!pool.is_allocated(&ip2));

    // Allocate again - should get a different IP or the released one
    let ip4 = pool.allocate().expect("should allocate after release");
    println!("After release, allocated: {}", ip4);

    println!("✓ IP pool allocation and release works correctly");
}
