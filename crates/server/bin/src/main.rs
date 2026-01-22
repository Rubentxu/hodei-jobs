//! Hodei Jobs gRPC Server
//!
//! Main entry point for the Hodei Jobs Platform server.

mod config;
mod startup;
// #[cfg(test)]
// mod tests_integration;

use clap::Parser;
use startup::{
    GracefulShutdown, GrpcServerConfig, ShutdownConfig, initialize_grpc_services, run,
    start_background_tasks, start_command_relay, start_job_coordinator, start_saga_consumers,
    start_signal_handler,
};
use std::sync::Arc;
use std::time::Duration;

use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// CLI arguments for hodei-server
#[derive(clap::Parser, Debug)]
#[command(name = "hodei-server")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Hodei Jobs Platform Server", long_about = None)]
struct Args {
    /// gRPC server port
    #[arg(short, long, default_value = "9090")]
    port: u16,

    /// Enable debug mode
    #[arg(short, long)]
    debug: bool,

    /// Shutdown timeout in seconds
    #[arg(long, default_value = "30")]
    shutdown_timeout: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let args = Args::parse();

    // Initialize logging
    setup_logging(args.debug);

    info!("╔═══════════════════════════════════════════════════════════════╗");
    info!("║           Hodei Jobs Platform - gRPC Server                   ║");
    info!("╚═══════════════════════════════════════════════════════════════╝");

    // Initialize graceful shutdown coordinator
    let shutdown_config =
        ShutdownConfig::default().with_timeout(Duration::from_secs(args.shutdown_timeout));
    let shutdown = GracefulShutdown::new(shutdown_config);

    // Start signal handlers (SIGTERM, SIGINT)
    start_signal_handler(&shutdown).await;
    info!("✓ Signal handlers started");

    // Get configuration
    let config = startup::StartupConfig::from_env()?;
    let grpc_config = GrpcServerConfig::from_env();

    info!("🚀 Starting Hodei Jobs Platform on {}", grpc_config.addr);

    // Run the application (connects to DB, runs migrations, connects to NATS, initializes services)
    let app_state = run(config.clone()).await?;

    // Initialize gRPC services first (before starting job coordinator)
    let event_bus = app_state.event_bus();
    let grpc_services = initialize_grpc_services(
        app_state.pool.clone(),
        app_state.worker_registry.clone(),
        app_state.job_repository.clone(),
        app_state.token_store.clone(),
        event_bus,
        app_state.outbox_repository.clone(),
    );
    info!("✓ gRPC services initialized");

    // Start JobCoordinator in background for reactive job processing
    let _coordinator_handle = start_job_coordinator(
        app_state.pool.clone(),
        app_state.worker_registry.clone(),
        app_state.job_repository.clone(),
        app_state.event_bus().clone(),
        app_state.token_store.clone(),
        app_state.lifecycle_manager.clone(),
        grpc_services.worker_agent_service.clone(),
        app_state.saga_orchestrator.clone(),
        config.clone(), // EPIC-94-C: Pass StartupConfig for get_worker_server_address()
        Arc::new(app_state.clone()), // EPIC-94-C: Pass AppState for v4.0 workflow coordinator
    )
    .await;

    // Store gRPC services in app_state for gRPC server
    let mut app_state = app_state;
    app_state.grpc_services = Some(Arc::new(grpc_services));

    // Start saga consumers for event-driven saga execution
    use hodei_server_infrastructure::persistence::postgres::{
        PostgresSagaOrchestrator, PostgresSagaRepository,
    };
    let saga_repository = Arc::new(PostgresSagaRepository::new(app_state.pool.clone()));
    let saga_orchestrator = Arc::new(PostgresSagaOrchestrator::new(saga_repository, None));

    // Start saga consumers (Cancellation + Cleanup)
    let _saga_consumers_handle = start_saga_consumers(
        &app_state.nats_event_bus,
        saga_orchestrator.clone(),
        app_state.pool.clone(),
    )
    .await;

    // ✅ Start WorkerEphemeralTerminatingConsumer for reactive worker cleanup
    let _worker_ephemeral_consumer_handle = startup::start_worker_ephemeral_terminating_consumer(
        &app_state.nats_event_bus,
        saga_orchestrator.clone(),
        app_state.pool.clone(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to start WorkerEphemeralTerminatingConsumer: {}", e))?;

    // Start background tasks (TimeoutChecker)
    let _background_tasks_handle = start_background_tasks(
        app_state.pool.clone(),
        saga_orchestrator.clone(),
        app_state.worker_registry.clone(),
        app_state.worker_provisioning.clone(),
    )
    .await;

    // ✅ Start Command Relay (EPIC-63)
    let _command_relay_handle = start_command_relay(
        app_state.pool.clone(),
        Arc::new(app_state.nats_event_bus.client().clone()),
    )
    .await;

    // ✅ Start SagaPoller for processing pending sagas (GAP 3 FIX)
    let _saga_poller_handle =
        startup::start_saga_poller(app_state.pool.clone(), saga_orchestrator.clone()).await;

    // ✅ Start NatsOutboxRelay for event publishing (GAP 4 FIX)
    let _nats_outbox_relay_handle =
        startup::start_nats_outbox_relay(app_state.pool.clone(), app_state.nats_event_bus.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start NatsOutboxRelay: {}", e))?;

    // ✅ Start ReactiveSagaProcessor for reactive saga processing (GAP 8 FIX)
    let _reactive_saga_processor_handle =
        startup::start_reactive_saga_processor(app_state.pool.clone(), saga_orchestrator.clone())
            .await;

    // Wait for shutdown signal
    info!("Server running. Waiting for shutdown signal (SIGTERM/SIGINT)...");

    // EPIC-94-D: Extract v4 worker handles before passing app_state to gRPC server
    let v4_shutdown_handles = app_state.v4_worker_shutdown_handles.clone();

    // Start the gRPC server with shutdown integration
    tokio::select! {
        result = startup::start_grpc_server(
            grpc_config.addr,
            app_state,
            grpc_config.enable_cors,
            grpc_config.enable_grpc_web,
        ) => {
            match result {
                Ok(_) => info!("gRPC server stopped"),
                Err(e) => tracing::error!("gRPC server error: {}", e),
            }
        }
        signal = shutdown.wait_for_signal() => {
            info!("Shutdown signal received: {}", signal);
        }
    }

    info!("Shutting down gracefully...");

    // EPIC-94-D: Shutdown v4.0 workers
    if let Some(handles) = v4_shutdown_handles {
        info!("Shutting down v4.0 workers...");
        handles.shutdown_all().await;
        info!("✓ v4.0 workers shut down");
    }

    Ok(())
}

/// Setup logging based on debug flag.
fn setup_logging(debug: bool) {
    let level = if debug { "debug" } else { "info" };
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_target(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}
