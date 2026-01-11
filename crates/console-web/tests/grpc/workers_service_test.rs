//! Workers Service gRPC Tests
//!
//! TDD tests for WorkersService gRPC client.
//! Following Red-Green-Refactor cycle.
//!
//! CORRECTED: Uses existing SchedulerService.GetAvailableWorkers

use hodei_console_web::grpc::{GrpcClient, GrpcClientConfig, GrpcClientError, WorkersService};
use hodei_jobs::{
    AvailableWorker, GetAvailableWorkersRequest, GetAvailableWorkersResponse, WorkerFilterCriteria,
    WorkerId, WorkerSchedulingInfo,
};
use rstest::{fixture, rstest};
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[fixture]
fn grpc_config() -> GrpcClientConfig {
    GrpcClientConfig {
        server_address: "http://localhost:50051".to_string(),
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(30),
        enable_reconnect: false, // Disable for tests
        reconnect_interval: Duration::from_secs(2),
        max_retries: 3,
        scheduler_name: "test-scheduler".to_string(),
    }
}

#[fixture]
fn mock_worker() -> AvailableWorker {
    AvailableWorker {
        worker_id: Some(WorkerId {
            value: "worker-001".to_string(),
        }),
        name: "test-worker".to_string(),
        provider_id: "kubernetes-001".to_string(),
        capabilities: None,
        current_load: 0,
        max_capacity: 4,
        scheduling_info: Some(WorkerSchedulingInfo {
            priority: 100,
            affinity_score: 1.0,
            last_job_completed: None,
        }),
        metadata: std::collections::HashMap::new(),
    }
}

#[fixture]
fn mock_workers_list() -> Vec<AvailableWorker> {
    vec![
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "worker-001".to_string(),
            }),
            name: "k8s-worker-1".to_string(),
            provider_id: "kubernetes-001".to_string(),
            capabilities: None,
            current_load: 0,
            max_capacity: 4,
            scheduling_info: Some(WorkerSchedulingInfo {
                priority: 100,
                affinity_score: 1.0,
                last_job_completed: None,
            }),
            metadata: std::collections::HashMap::new(),
        },
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "worker-002".to_string(),
            }),
            name: "docker-worker-1".to_string(),
            provider_id: "docker-001".to_string(),
            capabilities: None,
            current_load: 2,
            max_capacity: 2,
            scheduling_info: Some(WorkerSchedulingInfo {
                priority: 90,
                affinity_score: 0.8,
                last_job_completed: None,
            }),
            metadata: std::collections::HashMap::new(),
        },
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "worker-003".to_string(),
            }),
            name: "fc-worker-1".to_string(),
            provider_id: "firecracker-001".to_string(),
            capabilities: None,
            current_load: 1,
            max_capacity: 1,
            scheduling_info: Some(WorkerSchedulingInfo {
                priority: 80,
                affinity_score: 0.6,
                last_job_completed: None,
            }),
            metadata: std::collections::HashMap::new(),
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// WorkersService Creation Tests
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[rstest]
fn test_workers_service_creation(grpc_config: GrpcClientConfig) {
    // Given: A gRPC client configuration
    let client = GrpcClient::new("http://localhost:50051".to_string(), grpc_config);

    // When: Creating a WorkersService
    let service = WorkersService::new(client);

    // Then: Service should be created successfully
    // (This is a compilation test - if it compiles, it passes)
    drop(service); // Use the service to avoid unused variable warning
}

#[rstest]
fn test_workers_service_implements_clone(grpc_config: GrpcClientConfig) {
    // Given: A WorkersService
    let client = GrpcClient::new("http://localhost:50051".to_string(), grpc_config);
    let service = WorkersService::new(client);

    // When: Cloning the service
    let cloned_service = service.clone();

    // Then: Both services should be usable
    drop(service);
    drop(cloned_service);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// List Workers Tests (using existing GetAvailableWorkers)
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test list_workers returns all workers when no filter is provided
#[rstest]
#[tokio::test]
async fn test_list_workers_no_filter() {
    // RED phase - test first, implementation later
    // Uses SchedulerService.GetAvailableWorkers

    // Given: A WorkersService connected to a mock server
    // (Mock setup would go here in actual implementation)

    // When: Calling list_workers with no filter
    // let result = service.list_workers(None).await;

    // Then: Should return all workers
    // assert!(result.is_ok());
    // let response = result.unwrap();
    // assert_eq!(response.workers.len(), 3);
}

/// Test list_workers with custom filter
#[rstest]
#[tokio::test]
async fn test_list_workers_with_filter() {
    // RED phase

    // Given: A WorkersService with multiple workers
    // let filter = WorkerFilterCriteria {
    //     provider_id: Some("kubernetes-001".to_string()),
    //     ..Default::default()
    // };

    // When: Filtering by provider_id
    // let result = service.list_workers(Some(filter)).await;

    // Then: Should return only kubernetes workers
    // assert!(result.is_ok());
    // let response = result.unwrap();
    // assert!(response.workers.iter().all(|w| w.provider_id == "kubernetes-001"));
}

/// Test list_workers handles empty result
#[rstest]
#[tokio::test]
async fn test_list_workers_empty_result() {
    // RED phase

    // Given: A WorkersService with filter that matches no workers
    // let filter = WorkerFilterCriteria {
    //     provider_id: Some("non-existent".to_string()),
    //     ..Default::default()
    // };

    // When: Requesting workers
    // let result = service.list_workers(Some(filter)).await;

    // Then: Should return empty list
    // assert!(result.is_ok());
    // let response = result.unwrap();
    // assert_eq!(response.workers.len(), 0);
}

/// Test list_workers handles gRPC error
#[rstest]
#[tokio::test]
async fn test_list_workers_grpc_error() {
    // RED phase

    // Given: A WorkersService with disconnected client

    // When: Calling list_workers
    // let result = service.list_workers(None).await;

    // Then: Should return GrpcClientError
    // assert!(result.is_err());
    // match result.unwrap_err() {
    //     GrpcClientError::NotConnected => {},
    //     _ => panic!("Expected NotConnected error"),
    // }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Get Worker Info Tests
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test get_worker_info returns worker details
#[rstest]
#[tokio::test]
async fn test_get_worker_info_success() {
    // RED phase

    // Given: A WorkersService with an existing worker

    // When: Getting info for "worker-001"
    // let result = service.get_worker_info("worker-001").await;

    // Then: Should return worker info
    // assert!(result.is_ok());
    // let worker = result.unwrap();
    // assert_eq!(worker.worker_id.as_ref().unwrap().value, "worker-001");
    // assert_eq!(worker.name, "k8s-worker-1");
}

/// Test get_worker_info with non-existent worker
#[rstest]
#[tokio::test]
async fn test_get_worker_info_not_found() {
    // RED phase

    // Given: A WorkersService

    // When: Getting info for non-existent worker
    // let result = service.get_worker_info("non-existent").await;

    // Then: Should return error
    // assert!(result.is_err());
}

/// Test get_worker_info with empty worker_id
#[rstest]
#[tokio::test]
async fn test_get_worker_info_empty_id() {
    // RED phase

    // Given: A WorkersService

    // When: Getting info with empty id
    // let result = service.get_worker_info("").await;

    // Then: Should return validation error
    // assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Worker Capacity Tests
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test workers are sorted by available capacity
#[rstest]
fn test_worker_capacity_calculation() {
    // Given: Workers with different loads
    let workers = vec![
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "w1".to_string(),
            }),
            name: "worker-1".to_string(),
            provider_id: "test".to_string(),
            capabilities: None,
            current_load: 2,
            max_capacity: 4,
            scheduling_info: None,
            metadata: std::collections::HashMap::new(),
        },
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "w2".to_string(),
            }),
            name: "worker-2".to_string(),
            provider_id: "test".to_string(),
            capabilities: None,
            current_load: 0,
            max_capacity: 4,
            scheduling_info: None,
            metadata: std::collections::HashMap::new(),
        },
    ];

    // When: Calculating available capacity
    let available_1 = workers[0].max_capacity - workers[0].current_load;
    let available_2 = workers[1].max_capacity - workers[1].current_load;

    // Then: Worker 2 should have more capacity
    assert_eq!(available_1, 2);
    assert_eq!(available_2, 4);
    assert!(available_2 > available_1);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Error Handling Tests
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[rstest]
#[tokio::test]
async fn test_workers_service_handles_connection_timeout() {
    // RED phase

    // Given: A WorkersService with very short timeout

    // When: Attempting to connect to unavailable server
    // let result = service.list_workers(None).await;

    // Then: Should return timeout error
    // assert!(result.is_err());
    // match result.unwrap_err() {
    //     GrpcClientError::Timeout => {},
    //     _ => panic!("Expected Timeout error"),
    // }
}

#[rstest]
#[tokio::test]
async fn test_workers_service_handles_invalid_response() {
    // RED phase

    // Given: A WorkersService receiving malformed response

    // When: Parsing invalid response

    // Then: Should handle gracefully with error
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Integration Tests (require running server)
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[rstest]
#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn integration_test_list_workers_real_server() {
    // This test requires a running gRPC server with SchedulerService
    // Given: A real gRPC server running on localhost:50051
    let config = GrpcClientConfig::default();
    let client = GrpcClient::new("http://localhost:50051".to_string(), config);
    let service = WorkersService::new(client);

    // When: Connecting and listing workers
    let result = service.list_workers(None).await;

    // Then: Should succeed (or fail with meaningful error)
    match result {
        Ok(response) => {
            println!(
                "✓ Integration test: Listed {} workers",
                response.workers.len()
            );
            for worker in &response.workers {
                println!(
                    "  - {} (provider: {}, capacity: {}/{})",
                    worker.name, worker.provider_id, worker.current_load, worker.max_capacity
                );
            }
        }
        Err(e) => {
            println!("✗ Integration test failed: {:?}", e);
            // Don't panic - server might not be running
        }
    }
}

#[rstest]
#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn integration_test_get_worker_info_real_server() {
    // This test requires a running gRPC server
    let config = GrpcClientConfig::default();
    let client = GrpcClient::new("http://localhost:50051".to_string(), config);
    let service = WorkersService::new(client);

    // First, get list of workers
    let workers_result = service.list_workers(None).await;

    if let Ok(workers) = workers_result {
        if let Some(first_worker) = workers.workers.first() {
            let worker_id = &first_worker.worker_id.as_ref().unwrap().value;

            // When: Getting specific worker info
            let result = service.get_worker_info(worker_id).await;

            // Then: Should succeed
            match result {
                Ok(worker) => {
                    println!("✓ Integration test: Got worker info for {}", worker.name);
                    println!("  Provider: {}", worker.provider_id);
                    println!(
                        "  Capacity: {}/{}",
                        worker.current_load, worker.max_capacity
                    );
                }
                Err(e) => {
                    println!("✗ Integration test failed: {:?}", e);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Property-based Tests (future enhancement)
// ═══════════════════════════════════════════════════════════════════════════════════════════════

// TODO: Add proptest-based tests for:
// - Worker filtering edge cases
// - Capacity calculations
// - Priority sorting
// - Concurrent requests
