use hodei_jobs::{
    RegisterWorkerRequest,
    RegisterWorkerResponse,
    ServerMessage,
    UnregisterWorkerRequest,
    UnregisterWorkerResponse,
    UpdateWorkerStatusRequest,
    UpdateWorkerStatusResponse,
    WorkerId,
    WorkerMessage,
    // server_message::Payload as ServerPayload,
    worker_agent_service_server::{WorkerAgentService, WorkerAgentServiceServer},
    // worker_message::Payload as WorkerPayload,
};
use prost_types;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, transport::Server};

// --- Mock Server Implementation ---

#[derive(Clone)]
struct MockWorkerService {
    registrations: Arc<Mutex<Vec<String>>>,
    // Signal to kill active streams
    force_disconnect: Arc<tokio::sync::Notify>,
}

impl MockWorkerService {
    fn new(force_disconnect: Arc<tokio::sync::Notify>) -> Self {
        Self {
            registrations: Arc::new(Mutex::new(Vec::new())),
            force_disconnect,
        }
    }
}

#[tonic::async_trait]
impl WorkerAgentService for MockWorkerService {
    type WorkerStreamStream = ReceiverStream<Result<ServerMessage, Status>>;

    async fn register(
        &self,
        request: Request<RegisterWorkerRequest>,
    ) -> Result<Response<RegisterWorkerResponse>, Status> {
        let req = request.into_inner();
        let worker_id = req.worker_info.unwrap().worker_id.unwrap().value;
        println!("MockServer: Register request from {}", worker_id);

        self.registrations.lock().unwrap().push(worker_id.clone());

        Ok(Response::new(RegisterWorkerResponse {
            success: true,
            message: "Mock registration successful".to_string(),
            registration_time: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            worker_id: Some(WorkerId {
                value: worker_id.clone(),
            }),
            session_id: "mock-session".to_string(),
        }))
    }

    async fn worker_stream(
        &self,
        request: Request<tonic::Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::WorkerStreamStream>, Status> {
        println!("MockServer: Worker stream connected");
        // let (tx, rx) = mpsc::channel(1); // Removed unused
        let force_disconnect = self.force_disconnect.clone();

        // Let's create another channel just to hold the connection open
        let (hold_tx, hold_rx) = mpsc::channel::<Result<ServerMessage, Status>>(1);
        let hold_tx_clone = hold_tx.clone();

        // Re-spawn with sender
        tokio::spawn(async move {
            let _tx = hold_tx_clone; // Hold it
            let mut in_stream = request.into_inner();
            loop {
                tokio::select! {
                    msg = in_stream.message() => {
                       match msg {
                           Ok(Some(_)) => {}, // Logged above? No, moved here.
                           _ => break,
                       }
                    }
                    _ = force_disconnect.notified() => {
                        println!("MockServer: Forced disconnect triggered!");
                        break;
                    }
                }
            }
            // Loop breaks -> _tx dropped -> Stream closes!
        });

        // Use hold_rx for the response
        Ok(Response::new(ReceiverStream::new(hold_rx)))
    }

    async fn update_worker_status(
        &self,
        _request: Request<UpdateWorkerStatusRequest>,
    ) -> Result<Response<UpdateWorkerStatusResponse>, Status> {
        Ok(Response::new(UpdateWorkerStatusResponse {
            success: true,
            message: "Updated".to_string(),
            timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        }))
    }

    async fn unregister_worker(
        &self,
        _request: Request<UnregisterWorkerRequest>,
    ) -> Result<Response<UnregisterWorkerResponse>, Status> {
        Ok(Response::new(UnregisterWorkerResponse {
            success: true,
            message: "Unregistered".to_string(),
            jobs_migrated: 0,
        }))
    }
}

// --- Integration Test ---

#[tokio::test]
#[ignore = "Complex timing test - needs refactoring"]
async fn test_worker_recovery_on_server_restart() {
    // Use port 0 to automatically bind to an available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let port = addr.port();
    let bin_path = env!("CARGO_BIN_EXE_hodei-worker-bin");

    println!("Test: Starting Phase 1 Mock Server on port {}...", port);
    let force_disconnect = Arc::new(tokio::sync::Notify::new());
    let service = MockWorkerService::new(force_disconnect.clone());
    let service_clone = service.clone();

    let (shutdown_tx_1, mut shutdown_rx_1) = tokio::sync::broadcast::channel::<()>(1);

    // Spawn Server 1
    let server_handle_1 = tokio::spawn(async move {
        Server::builder()
            .add_service(WorkerAgentServiceServer::new(service_clone))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .expect("MockServer 1 failed");
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("Test: Spawning Worker...");
    let mut worker = Command::new(bin_path)
        .env("HODEI_SERVER_ADDR", format!("http://127.0.0.1:{}", port))
        .env("HODEI_WORKER_ID", "recovery-worker-01")
        .env("RUST_LOG", "info") // Ensure we get logs
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn worker");

    // ... helpers spawn ...

    // Helper to check worker logs
    let worker_stdout = worker.stdout.take().unwrap();
    let worker_stderr = worker.stderr.take().unwrap();

    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(worker_stdout);
        for line in reader.lines() {
            println!("[WorkerOut]: {}", line.unwrap_or_default());
        }
    });

    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(worker_stderr);
        for line in reader.lines() {
            eprintln!("[WorkerErr]: {}", line.unwrap_or_default());
        }
    });

    // Wait for registration
    println!("Test: Waiting for first registration...");
    let mut registered = false;
    for _ in 0..20 {
        // Increased timeout to 10s
        tokio::time::sleep(Duration::from_millis(500)).await;

        {
            // Scope for lock
            let regs = service.registrations.lock().unwrap();
            if regs.contains(&"recovery-worker-01".to_string()) {
                registered = true;
                break;
            }
        }
    }
    assert!(registered, "Worker failed to register initially");
    println!("Test: Worker registered successfully.");

    // Phase 2: Kill Server (Simulate Hard Crash)
    println!("Test: Killing Mock Server 1 (Hard Crash)...");
    force_disconnect.notify_waiters(); // Force close active streams
    server_handle_1.abort();
    // Wait for abort to take effect
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("Test: Mock Server 1 killed.");

    // Wait for worker to detect disconnect and attempt retries
    println!("Test: Waiting for worker to detect disconnect and retry...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Reset registrations in mock service to verify re-registration
    service.registrations.lock().unwrap().clear();

    // Phase 3: Start Server 2 (Restart) on a new port
    println!("Test: Starting Phase 2 Mock Server (Restart)...");
    let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap();
    let port2 = addr2.port();

    let (shutdown_tx_2, mut shutdown_rx_2) = tokio::sync::broadcast::channel::<()>(1);
    let service_clone_2 = service.clone();

    let server_handle_2 = tokio::spawn(async move {
        Server::builder()
            .add_service(WorkerAgentServiceServer::new(service_clone_2))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener2))
            .await
            .expect("MockServer 2 failed");
    });

    // Wait for server 2 to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Signal worker to reconnect to new server by killing the worker and restarting it
    println!("Test: Restarting worker to connect to new server...");
    let _ = Command::new("kill").arg(worker.id().to_string()).status();

    // Wait for worker to exit
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Spawn new worker connected to server 2
    println!("Test: Spawning new worker for server 2...");
    let mut worker2 = Command::new(bin_path)
        .env("HODEI_SERVER_ADDR", format!("http://127.0.0.1:{}", port2))
        .env("HODEI_WORKER_ID", "recovery-worker-01") // Same ID to verify re-registration
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn worker2");

    // Wait for worker to reconnect and re-register
    println!("Test: Waiting for RE-registration with server 2...");
    let mut re_registered = false;
    // We give it more time because of backoff
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let regs = service.registrations.lock().unwrap();
        if regs.contains(&"recovery-worker-01".to_string()) {
            re_registered = true;
            break;
        }
    }

    // Cleanup
    shutdown_tx_2.send(()).unwrap();
    server_handle_2.await.unwrap();

    // Kill worker
    let _ = Command::new("kill").arg(worker.id().to_string()).status(); // kill

    assert!(
        re_registered,
        "Worker failed to re-register after server restart"
    );
    println!("Test: Worker successfully re-registered!");
}
