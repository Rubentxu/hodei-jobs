//! Hodei Operator - Main Entry Point
//!
//! Kubernetes Operator for Hodei Jobs Platform
//! Manages Job, ProviderConfig, and WorkerPool CRDs

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use hodei_operator::OperatorState;
use hodei_operator::grpc::GrpcClient;
use hodei_operator::watcher::{
    create_job_controller, create_provider_config_controller, create_worker_pool_controller,
};
use kube::Client;
use std::sync::Arc;
use tokio::signal;
use tokio::time::{Duration, interval};
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

/// Hodei Kubernetes Operator
#[derive(Parser, Debug)]
#[command(name = "hodei-operator")]
#[command(author = "Hodei Team")]
#[command(version = "0.1.0")]
#[command(about = "Kubernetes Operator for Hodei Jobs Platform", long_about = None)]
struct Args {
    /// Hodei Server gRPC address
    #[arg(long, default_value = "http://hodei-server:50051")]
    pub server_addr: String,

    /// Authentication token for gRPC server
    #[arg(long)]
    pub token: Option<String>,

    /// Kubernetes namespace to watch
    #[arg(long, default_value = "default")]
    pub namespace: String,

    /// Log level
    #[arg(long, value_enum, default_value = "info")]
    pub log_level: LogLevel,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = match args.log_level {
        LogLevel::Trace => LevelFilter::TRACE,
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Error => LevelFilter::ERROR,
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(log_level.into()))
        .init();

    info!("Starting Hodei Operator");
    info!(server_addr = %args.server_addr, namespace = %args.namespace, "Operator configuration");

    let k8s_client = Client::try_default()
        .await
        .context("Failed to create Kubernetes client")?;
    info!("Connected to Kubernetes");

    let grpc_client = GrpcClient::new(args.server_addr.clone(), args.token.clone())
        .await
        .context("Failed to connect to Hodei Server")?;
    info!("Connected to Hodei Server at {}", args.server_addr);

    let state = Arc::new(OperatorState::new(
        grpc_client,
        k8s_client.clone(),
        args.namespace.clone(),
    ));

    // Spawn controllers
    let job_state = state.clone();
    tokio::spawn(async move {
        create_job_controller(job_state).await;
    });
    info!("Job controller started");

    let pc_state = state.clone();
    tokio::spawn(async move {
        create_provider_config_controller(pc_state).await;
    });
    info!("ProviderConfig controller started");

    let wp_state = state.clone();
    tokio::spawn(async move {
        create_worker_pool_controller(wp_state).await;
    });
    info!("WorkerPool controller started");

    // Health check loop
    let health_state = state.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = health_state.client.health_check().await {
                error!(error = %e, "Hodei Server health check failed");
            } else {
                info!("Hodei Server health check passed");
            }
        }
    });

    info!("Operator is running. Press Ctrl+C to stop.");
    let _ = signal::ctrl_c().await;
    info!("Shutting down operator...");

    Ok(())
}
