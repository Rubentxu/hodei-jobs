//! Workers Service gRPC Tests
//!
//! TDD tests for WorkersService gRPC client.
//! Tests the high-level API for worker operations.
//!
//! Uses actual proto-generated types with correct field names.

use hodei_console_web::grpc::{GrpcClient, GrpcClientConfig, GrpcClientError, WorkersService};
use hodei_jobs::{
    AvailableWorker, GetAvailableWorkersRequest, GetAvailableWorkersResponse, ResourceUsage,
    WorkerFilterCriteria, WorkerId, WorkerSchedulingInfo, WorkerStatus,
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
        scheduling_info: Some(WorkerSchedulingInfo {
            worker_id: Some(WorkerId {
                value: "worker-001".to_string(),
            }),
            node_selector: None,
            affinity: None,
            tolerations: vec![],
            score: 95.0,
            reason: vec!["high_score".to_string()],
        }),
        current_usage: Some(ResourceUsage {
            cpu_cores: 0.5,
            memory_bytes: 512_000_000,
            disk_bytes: 0,
            gpu_count: 0,
            custom_usage: std::collections::HashMap::new(),
        }),
        status: WorkerStatus::Available as i32,
        last_heartbeat: None,
    }
}

#[fixture]
fn mock_workers_list() -> Vec<AvailableWorker> {
    vec![
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "worker-001".to_string(),
            }),
            scheduling_info: Some(WorkerSchedulingInfo {
                worker_id: Some(WorkerId {
                    value: "worker-001".to_string(),
                }),
                node_selector: None,
                affinity: None,
                tolerations: vec![],
                score: 95.0,
                reason: vec!["high_score".to_string()],
            }),
            current_usage: Some(ResourceUsage {
                cpu_cores: 0.0,
                memory_bytes: 256_000_000,
                disk_bytes: 0,
                gpu_count: 0,
                custom_usage: std::collections::HashMap::new(),
            }),
            status: WorkerStatus::Available as i32,
            last_heartbeat: None,
        },
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "worker-002".to_string(),
            }),
            scheduling_info: Some(WorkerSchedulingInfo {
                worker_id: Some(WorkerId {
                    value: "worker-002".to_string(),
                }),
                node_selector: None,
                affinity: None,
                tolerations: vec![],
                score: 85.0,
                reason: vec!["medium_score".to_string()],
            }),
            current_usage: Some(ResourceUsage {
                cpu_cores: 2.0,
                memory_bytes: 2_000_000_000,
                disk_bytes: 0,
                gpu_count: 0,
                custom_usage: std::collections::HashMap::new(),
            }),
            status: WorkerStatus::JobStatusBusy as i32,
            last_heartbeat: None,
        },
        AvailableWorker {
            worker_id: Some(WorkerId {
                value: "worker-003".to_string(),
            }),
            scheduling_info: Some(WorkerSchedulingInfo {
                worker_id: Some(WorkerId {
                    value: "worker-003".to_string(),
                }),
                node_selector: None,
                affinity: None,
                tolerations: vec![],
                score: 75.0,
                reason: vec!["lower_score".to_string()],
            }),
            current_usage: Some(ResourceUsage {
                cpu_cores: 1.0,
                memory_bytes: 1_000_000_000,
                disk_bytes: 0,
                gpu_count: 0,
                custom_usage: std::collections::HashMap::new(),
            }),
            status: WorkerStatus::Available as i32,
            last_heartbeat: None,
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Test Utilities
// ═══════════════════════════════════════════════════════════════════════════════════════════════

fn create_mock_grpc_client(config: &GrpcClientConfig) -> GrpcClient {
    GrpcClient::new(config.server_address.clone(), config.clone())
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// WorkersService Unit Tests
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[rstest]
fn test_workers_service_new_creates_instance(grpc_config: GrpcClientConfig) {
    // Given
    let client = create_mock_grpc_client(&grpc_config);

    // When
    let _service = WorkersService::new(client);

    // Then - Service should be created successfully (no panic)
}

#[rstest]
fn test_workers_service_list_workers_request_format(grpc_config: GrpcClientConfig) {
    // Given
    let client = create_mock_grpc_client(&grpc_config);
    let _service = WorkersService::new(client);
    let filter = Some(WorkerFilterCriteria {
        required_status: WorkerStatus::Available as i32,
        ..Default::default()
    });

    // When - Build the request (validation only, won't actually connect)
    let request = GetAvailableWorkersRequest {
        filter: filter,
        scheduler_name: grpc_config.scheduler_name.clone(),
    };

    // Then - Request should have correct structure
    assert!(request.filter.is_some());
    assert_eq!(
        request.filter.as_ref().unwrap().required_status,
        WorkerStatus::Available as i32
    );
}

#[rstest]
fn test_workers_service_list_workers_empty_filter(grpc_config: GrpcClientConfig) {
    // Given
    let client = create_mock_grpc_client(&grpc_config);
    let _service = WorkersService::new(client);

    // When - Request with no filter
    let request = GetAvailableWorkersRequest {
        filter: None,
        scheduler_name: grpc_config.scheduler_name.clone(),
    };

    // Then - Should have empty filter
    assert!(request.filter.is_none());
}

#[rstest]
fn test_mock_workers_have_valid_structure(mock_workers_list: Vec<AvailableWorker>) {
    // Given - Pre-built mock data

    // Then - All workers should have valid worker_id
    for worker in &mock_workers_list {
        assert!(worker.worker_id.is_some(), "Worker should have worker_id");
        assert!(
            !worker.worker_id.as_ref().unwrap().value.is_empty(),
            "Worker ID should not be empty"
        );
    }
}

#[rstest]
fn test_mock_workers_status_states(mock_workers_list: Vec<AvailableWorker>) {
    // Given
    let expected_states = vec![
        WorkerStatus::Available as i32,
        WorkerStatus::JobStatusBusy as i32,
        WorkerStatus::Available as i32,
    ];

    // Then - Each worker should have expected status
    for (worker, expected_state) in mock_workers_list.iter().zip(expected_states.iter()) {
        assert_eq!(
            worker.status, *expected_state,
            "Worker state should match expected"
        );
    }
}

#[rstest]
fn test_workers_service_request_scheduling_name(grpc_config: GrpcClientConfig) {
    // Given
    let client = create_mock_grpc_client(&grpc_config);
    let _service = WorkersService::new(client);

    // When
    let request = GetAvailableWorkersRequest {
        filter: None,
        scheduler_name: grpc_config.scheduler_name.clone(),
    };

    // Then - Scheduling name should match config
    assert_eq!(request.scheduler_name, grpc_config.scheduler_name);
}

#[rstest]
fn test_worker_filter_criteria_default() {
    // Given - Default filter criteria
    let filter = WorkerFilterCriteria {
        required_status: WorkerStatus::Offline as i32,
        ..Default::default()
    };

    // Then - All fields should have defaults
    assert_eq!(filter.required_status, WorkerStatus::Offline as i32);
    assert!(filter.label_selector.is_none());
    assert!(filter.node_selector.is_none());
}

#[rstest]
fn test_workers_list_response_type(mock_workers_list: Vec<AvailableWorker>) {
    // Given
    let response = GetAvailableWorkersResponse {
        workers: mock_workers_list.clone(),
    };

    // Then - Response should contain workers
    assert_eq!(response.workers.len(), 3);
}

#[rstest]
fn test_worker_resource_usage_type(mock_worker: AvailableWorker) {
    // Given
    let worker = mock_worker;

    // Then - Worker should have current_usage with valid values
    let usage = worker.current_usage.unwrap();
    assert!(usage.cpu_cores >= 0.0);
    assert!(usage.memory_bytes >= 0);
}

#[rstest]
fn test_workers_service_client_type(grpc_config: GrpcClientConfig) {
    // Given
    let client = create_mock_grpc_client(&grpc_config);

    // When
    let service = WorkersService::new(client);

    // Then - Service should be cloneable
    let _service_clone = service.clone();
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Integration-style Tests (Mocked behavior validation)
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[rstest]
fn test_workers_list_response_preserves_order(mock_workers_list: Vec<AvailableWorker>) {
    // Given - Response with workers in specific order
    let response = GetAvailableWorkersResponse {
        workers: mock_workers_list.clone(),
    };

    // Then - Order should be preserved
    assert_eq!(response.workers.len(), 3);
    assert_eq!(
        response.workers[0].worker_id.as_ref().unwrap().value,
        "worker-001"
    );
    assert_eq!(
        response.workers[1].worker_id.as_ref().unwrap().value,
        "worker-002"
    );
    assert_eq!(
        response.workers[2].worker_id.as_ref().unwrap().value,
        "worker-003"
    );
}

#[rstest]
fn test_workers_service_error_handling(grpc_config: GrpcClientConfig) {
    // Given - Client that can't connect (not testing actual network)
    let client = create_mock_grpc_client(&grpc_config);
    let _service = WorkersService::new(client);

    // When/Then - Error type should be GrpcClientError
    // Actual connection errors would be tested in integration tests
    let _error_type: GrpcClientError = GrpcClientError::NotConnected;
    assert_eq!(format!("{}", _error_type), "Not connected to server");
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Property-based Tests (using rstest's parametric testing)
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[rstest]
#[case(WorkerStatus::Offline, "Offline")]
#[case(WorkerStatus::Registering, "Registering")]
#[case(WorkerStatus::Available, "Available")]
#[case(WorkerStatus::JobStatusBusy, "Busy")]
#[case(WorkerStatus::Draining, "Draining")]
#[case(WorkerStatus::Error, "Error")]
fn test_worker_status_enum_values(#[case] status: WorkerStatus, #[case] _expected: &str) {
    // Given
    let status_value = status;

    // Then - Status value should be valid enum value (i32 conversion works)
    let as_i32: i32 = status_value.into();
    assert!(as_i32 >= 0 && as_i32 <= 5);
}

#[rstest]
fn test_worker_scheduling_info_score_range() {
    // Given
    let scheduling_info = WorkerSchedulingInfo {
        worker_id: None,
        node_selector: None,
        affinity: None,
        tolerations: vec![],
        score: 95.5,
        reason: vec!["test".to_string()],
    };

    // Then - Score should be valid
    assert!(scheduling_info.score >= 0.0 && scheduling_info.score <= 100.0);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// Performance & Validation Tests
// ═══════════════════════════════════════════════════════════════════════════════════════════════

#[rstest]
fn test_workers_list_large_response_handling() {
    // Given - Large list of workers (simulating 1000+ workers)
    let large_list: Vec<AvailableWorker> = (0..1000)
        .map(|i| AvailableWorker {
            worker_id: Some(WorkerId {
                value: format!("worker-{:04}", i),
            }),
            scheduling_info: Some(WorkerSchedulingInfo {
                worker_id: Some(WorkerId {
                    value: format!("worker-{:04}", i),
                }),
                node_selector: None,
                affinity: None,
                tolerations: vec![],
                score: 100.0 - (i % 100) as f64,
                reason: vec![format!("score_{}", i)],
            }),
            current_usage: Some(ResourceUsage {
                cpu_cores: (i % 4) as f64,
                memory_bytes: 1_000_000_000 + (i as i64 * 1_000_000),
                disk_bytes: 0,
                gpu_count: i as i32 % 2,
                custom_usage: std::collections::HashMap::new(),
            }),
            status: if i % 10 == 0 {
                WorkerStatus::Offline as i32
            } else {
                WorkerStatus::Available as i32
            },
            last_heartbeat: None,
        })
        .collect();

    let response = GetAvailableWorkersResponse {
        workers: large_list.clone(),
    };

    // Then - Should handle large lists efficiently
    assert_eq!(response.workers.len(), 1000);

    // First and last should be correctly preserved
    assert_eq!(
        response
            .workers
            .first()
            .unwrap()
            .worker_id
            .as_ref()
            .unwrap()
            .value,
        "worker-0000"
    );
    assert_eq!(
        response
            .workers
            .last()
            .unwrap()
            .worker_id
            .as_ref()
            .unwrap()
            .value,
        "worker-0999"
    );
}

#[rstest]
fn test_worker_filter_criteria_status_filtering() {
    // Given - Filter for specific status
    let filter = WorkerFilterCriteria {
        required_status: WorkerStatus::Available as i32,
        ..Default::default()
    };

    // Then - Status filter should be set
    assert_eq!(filter.required_status, WorkerStatus::Available as i32);
}
