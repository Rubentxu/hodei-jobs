//! gRPC Server module
//!
//! This module is responsible for starting and managing the gRPC server.
//! It follows the Single Responsibility Principle - only handles gRPC server lifecycle.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;
use tonic_web::GrpcWebLayer;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use super::AppState;
use hodei_jobs::{
    job_execution_service_server::JobExecutionServiceServer,
    log_stream_service_server::LogStreamServiceServer,
    metrics_service_server::MetricsServiceServer,
    providers::provider_management_service_server::ProviderManagementServiceServer,
    scheduler_service_server::SchedulerServiceServer,
    worker_agent_service_server::WorkerAgentServiceServer,
};
use hodei_server_interface::grpc::LogStreamServiceGrpc;

/// Result type for gRPC server operations
pub type GrpcServerResult = Result<(), anyhow::Error>;

/// Configuration for the gRPC server
#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    /// Socket address to bind to
    pub addr: SocketAddr,
    /// Enable CORS for web clients
    pub enable_cors: bool,
    /// Enable gRPC-Web for browser clients
    pub enable_grpc_web: bool,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            addr: "0.0.0.0:9090".parse().unwrap(),
            enable_cors: true,
            enable_grpc_web: true,
        }
    }
}

impl GrpcServerConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        use std::env;

        let port: u16 = env::var("GRPC_PORT")
            .unwrap_or_else(|_| "9090".to_string())
            .parse()
            .unwrap_or(9090);

        let host = env::var("HODEI_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let addr = format!("{}:{}", host, port).parse().unwrap();

        Self {
            addr,
            enable_cors: true,
            enable_grpc_web: true,
        }
    }
}

/// Start the gRPC server with all services registered.
pub async fn start_grpc_server(
    addr: SocketAddr,
    app_state: AppState,
    enable_cors: bool,
    enable_grpc_web: bool,
) -> GrpcServerResult {
    info!("🚀 Starting gRPC server on {}", addr);

    // Build CORS layer if enabled
    let cors = if enable_cors {
        info!("✓ CORS enabled for web clients");
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
    };

    // Build gRPC-Web layer if enabled
    let grpc_web_layer = if enable_grpc_web {
        info!("✓ gRPC-Web enabled for browser clients");
        GrpcWebLayer::new()
    } else {
        GrpcWebLayer::new()
    };

    // Use gRPC services from app_state (already initialized for job coordinator)
    let services = app_state
        .grpc_services
        .as_ref()
        .expect("gRPC services must be initialized before starting the server");
    info!("Using pre-initialized gRPC services");

    // Add reflection service for debugging
    info!("Building reflection service...");
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(hodei_jobs::FILE_DESCRIPTOR_SET)
        .build_v1()
        .map_err(|e| anyhow::anyhow!("Failed to build reflection service: {}", e))?;

    info!("✓ Reflection service built");

    // Create LogStreamServiceGrpc wrapper
    let log_stream_grpc = LogStreamServiceGrpc::new(services.log_stream_service.clone());
    info!("✓ LogStreamServiceGrpc created");

    // Build server with all services
    let server = Server::builder()
        .accept_http1(true)
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(10)))
        .layer(cors)
        .layer(grpc_web_layer)
        .add_service(WorkerAgentServiceServer::new(
            services.worker_agent_service.clone(),
        ))
        .add_service(JobExecutionServiceServer::new(
            services.job_execution_service.clone(),
        ))
        .add_service(SchedulerServiceServer::new(
            services.scheduler_service.clone(),
        ))
        .add_service(ProviderManagementServiceServer::new(
            services.provider_management_service.clone(),
        ))
        .add_service(LogStreamServiceServer::new(log_stream_grpc))
        .add_service(MetricsServiceServer::new(services.metrics_service.clone()))
        .add_service(reflection_service);

    info!("✓ All gRPC services registered, binding to {}", addr);

    // Create TcpListener explicitly to ensure port is bound before serving
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

    let local_addr = listener
        .local_addr()
        .map_err(|e| anyhow::anyhow!("Failed to get local address: {}", e))?;

    info!("✓ Successfully bound to {}", local_addr);

    // Use serve_with_incoming with the already-bound listener
    let incoming = TcpListenerStream::new(listener);

    info!("✓ Starting gRPC server...");

    server.serve_with_incoming(incoming).await?;

    Ok(())
}
