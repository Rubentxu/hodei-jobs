mod config;

use crate::config::ServerConfig;
use sqlx::postgres::PgPool;
use tracing::{error, info};
use tracing_subscriber::FmtSubscriber;

use hodei_jobs::{
    audit_service_server::AuditServiceServer, worker_agent_service_server::WorkerAgentServiceServer,
};
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use hodei_server_infrastructure::persistence::postgres::{
    PostgresAuditRepository, PostgresJobQueue, PostgresJobRepository,
    PostgresProviderConfigRepository, PostgresWorkerRegistry,
};
use hodei_server_interface::grpc::{LogStreamService, WorkerAgentServiceImpl};
use std::sync::Arc;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize Logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Hodei Job Platform Server...");

    // 2. Load Config
    let config = ServerConfig::new().inspect_err(|e| error!("Failed to load config: {}", e))?;

    // 3. Initialize Infrastructure (Database)
    let pool = PgPool::connect(&config.database_url).await?;
    info!("Connected to database");

    // 4. Initialize Repositories
    let worker_registry = Arc::new(PostgresWorkerRegistry::new(pool.clone()));
    let job_repository = Arc::new(PostgresJobRepository::new(pool.clone()));
    let job_queue = Arc::new(PostgresJobQueue::new(pool.clone()));
    let audit_repository = Arc::new(PostgresAuditRepository::new(pool.clone()));
    let provider_config_repo = Arc::new(PostgresProviderConfigRepository::new(pool.clone()));

    // 5. Initialize Domain/Application Services
    // Note: This is an approximation of the wiring.
    // In a real scenario, you'd have a composition root.

    let event_bus = Arc::new(PostgresEventBus::new(pool.clone()));

    let log_service = LogStreamService::new();

    let worker_agent_service = WorkerAgentServiceImpl::with_registry_and_log_service(
        worker_registry.clone(),
        log_service.clone(),
        event_bus.clone(),
    );

    let addr = format!("0.0.0.0:{}", config.port).parse()?;
    info!("✓ Listening on {}", addr);

    Server::builder()
        .add_service(WorkerAgentServiceServer::new(worker_agent_service))
        .serve(addr)
        .await?;

    Ok(())
}
