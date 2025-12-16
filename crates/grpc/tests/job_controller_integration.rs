//! Integration test for JobController + gRPC dispatch

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time::timeout;
use tonic::transport::{Channel, Server};

use hodei_jobs::{
    RegisterWorkerRequest, WorkerHeartbeat, WorkerId, WorkerInfo, WorkerMessage,
    server_message::Payload as ServerPayload,
    worker_agent_service_client::WorkerAgentServiceClient,
    worker_agent_service_server::WorkerAgentServiceServer,
    worker_message::Payload as WorkerPayload,
};

use hodei_jobs_application::{job_controller::JobController, smart_scheduler::SchedulerConfig};
use hodei_jobs_domain::job_execution::{Job, JobSpec};
use hodei_jobs_domain::shared_kernel::ProviderId;
use hodei_jobs_domain::worker::{ProviderType, WorkerHandle, WorkerSpec as DomainWorkerSpec};
use hodei_jobs_infrastructure::repositories::{
    InMemoryJobQueue, InMemoryJobRepository, InMemoryWorkerRegistry,
};

use hodei_jobs_grpc::services::WorkerAgentServiceImpl;
use hodei_jobs_grpc::worker_command_sender::GrpcWorkerCommandSender;

use futures::stream::BoxStream;
use hodei_jobs_domain::DomainEvent;
use hodei_jobs_domain::event_bus::{EventBus, EventBusError};
use std::sync::Mutex;

struct MockEventBus {
    published: Arc<Mutex<Vec<DomainEvent>>>,
}
impl MockEventBus {
    fn new() -> Self {
        Self {
            published: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
#[tonic::async_trait]
impl EventBus for MockEventBus {
    async fn publish(&self, event: &DomainEvent) -> std::result::Result<(), EventBusError> {
        self.published.lock().unwrap().push(event.clone());
        Ok(())
    }
    async fn subscribe(
        &self,
        _topic: &str,
    ) -> std::result::Result<
        BoxStream<'static, std::result::Result<DomainEvent, EventBusError>>,
        EventBusError,
    > {
        Err(EventBusError::SubscribeError(
            "Mock not implemented".to_string(),
        ))
    }
}

async fn start_test_server(service: WorkerAgentServiceImpl) -> (SocketAddr, oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        Server::builder()
            .add_service(WorkerAgentServiceServer::new(service))
            .serve_with_shutdown(addr, async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    (addr, shutdown_tx)
}

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
async fn job_controller_dispatches_run_job_to_connected_worker() {
    // Infrastructure for controller (in-memory is OK for tests)
    let job_repository = Arc::new(InMemoryJobRepository::new())
        as Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
    let job_queue =
        Arc::new(InMemoryJobQueue::new()) as Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;
    let worker_registry = Arc::new(InMemoryWorkerRegistry::new())
        as Arc<dyn hodei_jobs_domain::worker_registry::WorkerRegistry>;

    // Create worker service with registry+repo so Register/Heartbeat and JobResult update persistence
    let log_service = hodei_jobs_grpc::services::LogStreamService::new();
    let event_bus = Arc::new(MockEventBus::new());
    let worker_service = WorkerAgentServiceImpl::with_registry_job_repository_and_log_service(
        worker_registry.clone(),
        job_repository.clone(),
        log_service,
        event_bus.clone(),
    );

    // Start gRPC server
    let (addr, _shutdown) = start_test_server(worker_service.clone()).await;
    let mut client = create_client(addr).await;

    // Pre-provision/register worker in registry (simulates provider provisioning)
    let worker_uuid = uuid::Uuid::new_v4();
    let worker_id = hodei_jobs_domain::shared_kernel::WorkerId(worker_uuid);
    let handle = WorkerHandle::new(
        worker_id.clone(),
        "resource".to_string(),
        ProviderType::Docker,
        ProviderId::new(),
    );
    let mut spec = DomainWorkerSpec::new(
        "hodei-jobs-worker:latest".to_string(),
        format!("http://{}", addr),
    );
    spec.worker_id = worker_id.clone();

    worker_registry.register(handle, spec).await.unwrap();

    // OTP and register
    let otp = worker_service
        .generate_otp(&worker_uuid.to_string())
        .await
        .expect("generate_otp should succeed");
    let reg_request = RegisterWorkerRequest {
        auth_token: otp,
        session_id: String::new(),
        worker_info: Some(WorkerInfo {
            worker_id: Some(WorkerId {
                value: worker_uuid.to_string(),
            }),
            ..Default::default()
        }),
    };
    client.register(reg_request).await.unwrap();

    // Connect stream and send heartbeat to establish channel
    let (tx, rx) = tokio::sync::mpsc::channel::<WorkerMessage>(10);
    let heartbeat = WorkerMessage {
        payload: Some(WorkerPayload::Heartbeat(WorkerHeartbeat {
            worker_id: Some(WorkerId {
                value: worker_uuid.to_string(),
            }),
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

    // Wait ACK
    let _ = timeout(Duration::from_secs(2), stream.message()).await;

    // Create job and enqueue
    let job_id = hodei_jobs_domain::shared_kernel::JobId::new();
    let job = Job::new(
        job_id.clone(),
        JobSpec::new(vec!["echo".to_string(), "hi".to_string()]),
    );
    job_repository.save(&job).await.unwrap();
    job_queue.enqueue(job).await.unwrap();

    // Controller that dispatches via gRPC sender
    let sender = Arc::new(GrpcWorkerCommandSender::new(worker_service.clone()))
        as Arc<dyn hodei_jobs_application::worker_command_sender::WorkerCommandSender>;

    let controller = JobController::new(
        job_queue,
        job_repository.clone(),
        worker_registry,
        SchedulerConfig::default(),
        sender,
        event_bus.clone(),
    );

    let processed = controller.run_once().await.unwrap();
    assert_eq!(processed, 1);

    // Worker should receive RunJob
    let msg = timeout(Duration::from_secs(2), stream.message()).await;
    assert!(msg.is_ok());

    if let Ok(Ok(Some(server_msg))) = msg {
        match server_msg.payload {
            Some(ServerPayload::RunJob(run)) => {
                assert_eq!(run.job_id, job_id.to_string());
            }
            other => panic!("Expected RunJob, got: {:?}", other),
        }
    }
}
