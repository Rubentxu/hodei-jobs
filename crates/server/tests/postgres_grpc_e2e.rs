use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tonic::transport::{Channel, Server};

use hodei_jobs::{
    GetAvailableWorkersRequest, GetQueueStatusRequest, JobDefinition, ScheduleJobRequest, WorkerId,
    scheduler_service_client::SchedulerServiceClient,
    scheduler_service_server::SchedulerServiceServer,
    worker_agent_service_server::WorkerAgentServiceServer,
};

use hodei_jobs_application::jobs::create::CreateJobUseCase;
use hodei_jobs_application::smart_scheduler::SchedulerConfig;
use hodei_jobs_domain::shared_kernel::{ProviderId, WorkerState};
use hodei_jobs_domain::workers::{ProviderType, WorkerHandle, WorkerSpec};
use hodei_jobs_grpc::services::{SchedulerServiceImpl, WorkerAgentServiceImpl};
use hodei_jobs_infrastructure::persistence::{
    DatabaseConfig, PostgresJobQueue, PostgresJobRepository, PostgresWorkerRegistry,
};

mod common;

async fn start_test_server(db_url: String) -> (SocketAddr, oneshot::Sender<()>) {
    let db_config = DatabaseConfig {
        url: db_url,
        max_connections: 5,
        connection_timeout: Duration::from_secs(10),
    };

    let job_repository = PostgresJobRepository::connect(&db_config).await.unwrap();
    job_repository.run_migrations().await.unwrap();

    let job_queue = PostgresJobQueue::connect(&db_config).await.unwrap();
    job_queue.run_migrations().await.unwrap();

    let worker_registry = PostgresWorkerRegistry::connect(&db_config).await.unwrap();
    worker_registry.run_migrations().await.unwrap();

    let job_repository =
        Arc::new(job_repository) as Arc<dyn hodei_jobs_domain::jobs::JobRepository>;
    let job_queue = Arc::new(job_queue) as Arc<dyn hodei_jobs_domain::jobs::JobQueue>;
    let worker_registry =
        Arc::new(worker_registry) as Arc<dyn hodei_jobs_domain::workers::WorkerRegistry>;

    // Register a worker in DB so scheduler can assign.
    let worker_spec = WorkerSpec::new(
        "hodei-jobs-worker:latest".to_string(),
        "http://localhost:50051".to_string(),
    );
    let worker_id = worker_spec.worker_id.clone();
    let handle = WorkerHandle::new(
        worker_id.clone(),
        "container-test".to_string(),
        ProviderType::Docker,
        ProviderId::new(),
    );

    let mut worker = worker_registry.register(handle, worker_spec).await.unwrap();
    worker.mark_starting().unwrap();
    worker.mark_ready().unwrap();
    worker_registry
        .update_state(worker.id(), WorkerState::Ready)
        .await
        .unwrap();

    let event_bus = Arc::new(common::MockEventBus);
    let create_job_usecase =
        CreateJobUseCase::new(job_repository.clone(), event_bus);
    let scheduler_service = SchedulerServiceImpl::new(
        Arc::new(create_job_usecase),
        job_repository,
        job_queue,
        worker_registry,
        SchedulerConfig::default(),
    );

    // include WorkerAgentService so reflection of server shape stays similar; not used in this test
    let worker_agent_service = WorkerAgentServiceImpl::new();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        Server::builder()
            .add_service(SchedulerServiceServer::new(scheduler_service))
            .add_service(WorkerAgentServiceServer::new(worker_agent_service))
            .serve_with_shutdown(addr, async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    (addr, shutdown_tx)
}

async fn create_scheduler_client(addr: SocketAddr) -> SchedulerServiceClient<Channel> {
    let endpoint = format!("http://{}", addr);
    let channel = Channel::from_shared(endpoint)
        .unwrap()
        .connect()
        .await
        .unwrap();
    SchedulerServiceClient::new(channel)
}

#[tokio::test]
#[ignore = "Requires Docker/Testcontainers (Postgres)"]
async fn test_scheduler_e2e_postgres_assigns_job_to_worker() {
    let Ok(db) = common::get_postgres_context().await else {
        eprintln!("Skipping test: Docker/Testcontainers not available");
        return;
    };

    let (addr, _shutdown) = start_test_server(db.connection_string.clone()).await;
    let mut client = create_scheduler_client(addr).await;

    let job_definition = JobDefinition {
        job_id: None,
        name: "e2e-job".to_string(),
        description: "".to_string(),
        command: "echo".to_string(),
        arguments: vec!["hello".to_string()],
        environment: std::collections::HashMap::new(),
        requirements: None,
        scheduling: None,
        selector: None,
        tolerations: vec![],
        timeout: None,
        tags: vec![],
    };

    let schedule_resp = client
        .schedule_job(ScheduleJobRequest {
            job_definition: Some(job_definition),
            requested_by: "test".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(schedule_resp.success);
    let decision = schedule_resp.decision.expect("decision");
    assert!(decision.selected_worker_id.is_some());
    assert!(decision.execution_id.is_some());

    let queue_status = client
        .get_queue_status(GetQueueStatusRequest {
            scheduler_name: "default".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    let status = queue_status.status.expect("status");
    assert_eq!(status.pending_jobs, 0);

    let workers = client
        .get_available_workers(GetAvailableWorkersRequest {
            filter: None,
            scheduler_name: "default".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    // Worker should exist and be BUSY after assignment.
    assert!(workers.workers.iter().any(|w| {
        w.worker_id
            .as_ref()
            .map(|id| !id.value.is_empty())
            .unwrap_or(false)
            && w.status == 3 // BUSY
    }));

    // Compile-time use of WorkerId to ensure proto types are linked
    let _ = WorkerId {
        value: "".to_string(),
    };
}
