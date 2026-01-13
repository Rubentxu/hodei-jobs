//! Hodei Jobs gRPC Server
//!
//! Main entry point for the Hodei Jobs Platform server.

mod config;
mod startup;

use clap::Parser;
use startup::run;

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let args = Args::parse();

    // Initialize logging
    setup_logging(args.debug);

    // Build configuration
    let config = startup::StartupConfig::from_env()?;

    // Run the application
    run(config).await?;

    // Keep the application running
    keep_running().await;

    Ok(())
}

/// Setup logging based on debug flag.
fn setup_logging(debug: bool) {
    use tracing_subscriber::{EnvFilter, FmtSubscriber};

    let level = if debug { "debug" } else { "info" };
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_target(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}

/// Keep the application running until interrupted.
async fn keep_running() {
    // Wait for Ctrl+C
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!("Failed to setup signal handler: {}", e);
    }

    tracing::info!("Shutting down gracefully...");
}
