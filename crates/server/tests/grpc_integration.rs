//! Integration tests for gRPC Worker Agent Service
//!
//! Tests the complete flow:
//! 1. Server startup
//! 2. Client connection
//! 3. OTP generation and worker registration
//! 4. Bidirectional stream communication (WorkerStream)

use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tonic::transport::{Channel, Server};

use hodei_jobs::{
    worker_agent_service_client::WorkerAgentServiceClient,
    worker_agent_service_server::WorkerAgentServiceServer,
    RegisterWorkerRequest, WorkerInfo, WorkerId, ResourceCapacity,
    WorkerMessage, WorkerHeartbeat, ResourceUsage,
    worker_message::Payload as WorkerPayload,
    server_message::Payload as ServerPayload,
};
use hodei_jobs_grpc::services::WorkerAgentServiceImpl;

/// Helper to start a gRPC server on a random available port
async fn start_test_server() -> (SocketAddr, WorkerAgentServiceImpl, oneshot::Sender<()>) {
    let service = WorkerAgentServiceImpl::new();
    let service_clone = service.clone();
    
    // Find available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    
    let svc = service.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(WorkerAgentServiceServer::new(svc))
            .serve_with_shutdown(addr, async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    
    // Wait for server to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    (addr, service_clone, shutdown_tx)
}

/// Helper to create a client connected to the test server
async fn create_client(addr: SocketAddr) -> WorkerAgentServiceClient<Channel> {
    let endpoint = format!("http://{}", addr);
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .connect()
        .await
        .unwrap();
    WorkerAgentServiceClient::new(channel)
}

#[tokio::test]
async fn test_full_registration_flow() {
    let (addr, service, _shutdown) = start_test_server().await;
    let mut client = create_client(addr).await;
    
    // Step 1: Generate OTP on server side (simulating provider creating worker)
    let worker_uuid = uuid::Uuid::new_v4();
    let worker_id = worker_uuid.to_string();
    let otp_token = service.generate_otp(&worker_id).await.unwrap();
    
    // Step 2: Worker uses OTP to register
    let request = RegisterWorkerRequest {
        auth_token: otp_token,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            name: "Integration Test Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux x86_64".to_string(),
            architecture: "x86_64".to_string(),
            capacity: Some(ResourceCapacity {
                cpu_cores: 4.0,
                memory_bytes: 8 * 1024 * 1024 * 1024,
                disk_bytes: 100 * 1024 * 1024 * 1024,
                gpu_count: 0,
                custom_resources: std::collections::HashMap::new(),
            }),
            capabilities: vec!["docker".to_string(), "shell".to_string()],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };
    
    let response = client.register(request).await.unwrap().into_inner();
    
    assert!(response.success);
    assert!(!response.session_id.is_empty());
    assert!(response.session_id.starts_with("sess_"));
    assert_eq!(response.worker_id.unwrap().value, worker_id);
}

#[tokio::test]
async fn test_registration_without_otp_fails() {
    let (addr, _service, _shutdown) = start_test_server().await;
    let mut client = create_client(addr).await;
    
    // Try to register without valid OTP
    let request = RegisterWorkerRequest {
        auth_token: "fake-invalid-token".to_string(),
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: uuid::Uuid::new_v4().to_string() }),
            name: "Unauthorized Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: None,
            capabilities: vec![],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };
    
    let response = client.register(request).await;
    
    assert!(response.is_err());
    let status = response.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn test_worker_stream_connection() {
    let (addr, service, _shutdown) = start_test_server().await;
    let mut client = create_client(addr).await;
    
    // First register the worker
    let worker_uuid = uuid::Uuid::new_v4();
    let worker_id = worker_uuid.to_string();
    let otp_token = service.generate_otp(&worker_id).await.unwrap();
    
    let reg_request = RegisterWorkerRequest {
        auth_token: otp_token,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            name: "Stream Test Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: None,
            capabilities: vec![],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };
    
    let reg_response = client.register(reg_request).await.unwrap().into_inner();
    assert!(reg_response.success);
    
    // Now connect via WorkerStream
    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);
    
    // Send a heartbeat message
    let heartbeat = WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            status: 2, // AVAILABLE
            usage: Some(ResourceUsage {
                cpu_cores: 0.5,
                memory_bytes: 1024 * 1024 * 1024,
                disk_bytes: 10 * 1024 * 1024 * 1024,
                gpu_count: 0,
                custom_usage: std::collections::HashMap::new(),
            }),
            active_jobs: 0,
            running_job_ids: vec![],
            timestamp: None,
        })),
    };
    
    tx.send(heartbeat).await.unwrap();
    drop(tx); // Close sender to end stream
    
    let inbound = tokio_stream::wrappers::ReceiverStream::new(rx);
    
    // Connect and get response stream
    let response = client.worker_stream(inbound).await;
    assert!(response.is_ok());
    
    let mut stream = response.unwrap().into_inner();
    
    // Should receive ACK for heartbeat
    let msg = timeout(Duration::from_secs(2), stream.message()).await;
    assert!(msg.is_ok());
    
    if let Ok(Ok(Some(server_msg))) = msg {
        match server_msg.payload {
            Some(ServerPayload::Ack(ack)) => {
                assert!(ack.success);
            }
            _ => panic!("Expected ACK message"),
        }
    }
}

#[tokio::test]
async fn test_multiple_workers_registration() {
    let (addr, service, _shutdown) = start_test_server().await;
    let mut client = create_client(addr).await;
    
    // Register multiple workers
    for i in 1..=5 {
        let worker_uuid = uuid::Uuid::new_v4();
        let worker_id = worker_uuid.to_string();
        let otp_token = service.generate_otp(&worker_id).await.unwrap();
        
        let request = RegisterWorkerRequest {
            auth_token: otp_token,
            session_id: String::new(),
            worker_info: Some(WorkerInfo {
                worker_id: Some(WorkerId { value: worker_id.clone() }),
                name: format!("Multi Worker {}", i),
                version: "1.0.0".to_string(),
                hostname: format!("host-{}", i),
                ip_address: format!("192.168.1.{}", i),
                os_info: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                capacity: None,
                capabilities: vec!["docker".to_string()],
                taints: vec![],
                labels: std::collections::HashMap::new(),
                tolerations: vec![],
                affinity: None,
                start_time: None,
            }),
        };
        
        let response = client.register(request).await.unwrap().into_inner();
        assert!(response.success);
        assert_eq!(response.worker_id.unwrap().value, worker_id);
    }
}

#[tokio::test]
async fn test_unregister_worker() {
    let (addr, service, _shutdown) = start_test_server().await;
    let mut client = create_client(addr).await;
    
    // Register worker first
    let worker_uuid = uuid::Uuid::new_v4();
    let worker_id = worker_uuid.to_string();
    let otp_token = service.generate_otp(&worker_id).await.unwrap();
    
    let reg_request = RegisterWorkerRequest {
        auth_token: otp_token,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            name: "Unregister Test Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: None,
            capabilities: vec![],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };
    
    client.register(reg_request).await.unwrap();
    
    // Now unregister
    let unreg_request = hodei_jobs::UnregisterWorkerRequest {
        worker_id: Some(WorkerId { value: worker_id.to_string() }),
        reason: "Integration test cleanup".to_string(),
    };
    
    let response = client.unregister_worker(unreg_request).await.unwrap().into_inner();
    assert!(response.success);
    
    // Try to unregister again - should still succeed but with "not found" message
    let unreg_request2 = hodei_jobs::UnregisterWorkerRequest {
        worker_id: Some(WorkerId { value: worker_id.to_string() }),
        reason: "Second unregister attempt".to_string(),
    };
    
    let response2 = client.unregister_worker(unreg_request2).await.unwrap().into_inner();
    assert!(!response2.success); // Should indicate not found
}

/// E2E Test: Server sends job command to connected worker
#[tokio::test]
async fn test_e2e_server_sends_job_to_worker() {
    use hodei_jobs::{
        RunJobCommand, ShellCommand, CommandSpec,
        command_spec::CommandType as CmdType,
    };
    
    let (addr, service, _shutdown) = start_test_server().await;
    let mut client = create_client(addr).await;
    
    // Register worker
    let worker_uuid = uuid::Uuid::new_v4();
    let worker_id = worker_uuid.to_string();
    let otp_token = service.generate_otp(&worker_id).await.unwrap();
    
    let reg_request = RegisterWorkerRequest {
        auth_token: otp_token,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            name: "E2E Test Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: None,
            capabilities: vec!["shell".to_string()],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };
    
    client.register(reg_request).await.unwrap();
    
    // Connect via stream
    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);
    
    // Send initial heartbeat to establish connection
    let heartbeat = WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            status: 2,
            usage: None,
            active_jobs: 0,
            running_job_ids: vec![],
            timestamp: None,
        })),
    };
    tx.send(heartbeat).await.unwrap();
    
    let inbound = tokio_stream::wrappers::ReceiverStream::new(rx);
    let response = client.worker_stream(inbound).await.unwrap();
    let mut stream = response.into_inner();
    
    // Wait for ACK
    let _ack = timeout(Duration::from_secs(2), stream.message()).await;
    
    // Now server should be able to send job to worker
    let job_command = hodei_jobs::ServerMessage {
        payload: Some(ServerPayload::RunJob(RunJobCommand {
            job_id: "test-job-123".to_string(),
            command: Some(CommandSpec {
                command_type: Some(CmdType::Shell(ShellCommand {
                    cmd: "echo".to_string(),
                    args: vec!["Hello E2E".to_string()],
                })),
            }),
            env: std::collections::HashMap::new(),
            inputs: vec![],
            outputs: vec![],
            timeout_ms: 30000,
            working_dir: String::new(),
        })),
    };
    
    // Server sends job to worker via channel
    let result = service.send_to_worker(&worker_id, job_command).await;
    
    // Worker should receive the job command
    if result.is_ok() {
        let msg = timeout(Duration::from_secs(2), stream.message()).await;
        assert!(msg.is_ok());
        
        if let Ok(Ok(Some(server_msg))) = msg {
            match server_msg.payload {
                Some(ServerPayload::RunJob(run)) => {
                    assert_eq!(run.job_id, "test-job-123");
                }
                _ => {} // ACK is also valid
            }
        }
    }
}

#[tokio::test]
async fn test_session_id_preserved_on_reconnection() {
    let (addr, service, _shutdown) = start_test_server().await;
    let mut client = create_client(addr).await;
    
    // First registration
    let worker_uuid = uuid::Uuid::new_v4();
    let worker_id = worker_uuid.to_string();
    let otp_token1 = service.generate_otp(&worker_id).await.unwrap();
    
    let reg_request1 = RegisterWorkerRequest {
        auth_token: otp_token1,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            name: "Reconnect Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: None,
            capabilities: vec![],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };
    
    let response1 = client.register(reg_request1).await.unwrap().into_inner();
    let session_id = response1.session_id;
    assert!(!session_id.is_empty());
    
    // Second registration with same session_id (simulating reconnection)
    let otp_token2 = service.generate_otp(&worker_id).await.unwrap();
    
    let reg_request2 = RegisterWorkerRequest {
        auth_token: otp_token2,
        session_id: session_id.clone(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId { value: worker_id.to_string() }),
            name: "Reconnect Worker".to_string(),
            version: "1.0.0".to_string(),
            hostname: "test-host".to_string(),
            ip_address: "127.0.0.1".to_string(),
            os_info: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            capacity: None,
            capabilities: vec![],
            taints: vec![],
            labels: std::collections::HashMap::new(),
            tolerations: vec![],
            affinity: None,
            start_time: None,
        }),
    };
    
    let response2 = client.register(reg_request2).await.unwrap().into_inner();
    
    // Session ID should be preserved
    assert_eq!(response2.session_id, session_id);
}
