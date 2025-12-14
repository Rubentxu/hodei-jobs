//! Example gRPC Server for Hodei Job Platform
//!
//! This example demonstrates how to start a basic gRPC server with all services.

use hodei_jobs_grpc::services::{
    WorkerAgentServiceImpl, JobExecutionServiceImpl,
    MetricsServiceImpl, SchedulerServiceImpl,
};
use hodei_jobs_application::job_execution_usecases::{CancelJobUseCase, CreateJobUseCase};
use hodei_jobs_application::smart_scheduler::SchedulerConfig;
use hodei_jobs_infrastructure::repositories::{
    InMemoryJobQueue, InMemoryJobRepository, InMemoryWorkerRegistry,
};
use hodei_jobs::{
    worker_agent_service_server::WorkerAgentServiceServer,
    job_execution_service_server::JobExecutionServiceServer,
    metrics_service_server::MetricsServiceServer,
    scheduler_service_server::SchedulerServiceServer,
};
use tonic::transport::Server;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let addr = "0.0.0.0:50051".parse()?;
    info!("Starting Hodei Job Platform gRPC Server on {}", addr);
    
    let worker_service = WorkerAgentServiceImpl::new();

    let job_repository = std::sync::Arc::new(InMemoryJobRepository::new())
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
    let job_queue = std::sync::Arc::new(InMemoryJobQueue::new())
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;
    let worker_registry = std::sync::Arc::new(InMemoryWorkerRegistry::new())
        as std::sync::Arc<dyn hodei_jobs_domain::worker_registry::WorkerRegistry>;

    let create_job_usecase = CreateJobUseCase::new(job_repository.clone(), job_queue);
    let cancel_job_usecase = CancelJobUseCase::new(job_repository.clone());

    let job_service = JobExecutionServiceImpl::new(
        std::sync::Arc::new(create_job_usecase),
        std::sync::Arc::new(cancel_job_usecase),
        job_repository,
        worker_registry,
    );
    let metrics_service = MetricsServiceImpl::new();

    let scheduler_job_repository = std::sync::Arc::new(InMemoryJobRepository::new())
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
    let scheduler_job_queue = std::sync::Arc::new(InMemoryJobQueue::new())
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;
    let scheduler_worker_registry = std::sync::Arc::new(InMemoryWorkerRegistry::new())
        as std::sync::Arc<dyn hodei_jobs_domain::worker_registry::WorkerRegistry>;
    let scheduler_create_job_usecase =
        CreateJobUseCase::new(scheduler_job_repository.clone(), scheduler_job_queue.clone());

    let scheduler_service = SchedulerServiceImpl::new(
        std::sync::Arc::new(scheduler_create_job_usecase),
        scheduler_job_repository,
        scheduler_job_queue,
        scheduler_worker_registry,
        SchedulerConfig::default(),
    );
    
    Server::builder()
        .add_service(WorkerAgentServiceServer::new(worker_service))
        .add_service(JobExecutionServiceServer::new(job_service))
        .add_service(MetricsServiceServer::new(metrics_service))
        .add_service(SchedulerServiceServer::new(scheduler_service))
        .serve(addr)
        .await?;

    Ok(())
}
