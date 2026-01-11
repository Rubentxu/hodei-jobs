//! Hodei Console Web - SSR Server Binary
//!
//! Production-ready Axum server for server-side rendering of the
//! Hodei Jobs Administration Console.

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use hodei_console_web::server;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;

#[derive(Parser, Debug)]
#[command(name = "hodei-console-web")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Bind address for the HTTP server (e.g., "0.0.0.0:3000")
    #[arg(short, long, default_value = "0.0.0.0:3000")]
    bind: String,

    /// gRPC server address for proxying requests
    #[arg(short, long, default_value = "http://localhost:50051")]
    grpc_address: String,

    /// Enable gRPC proxy for development
    #[arg(long, default_value = "false")]
    enable_grpc_proxy: bool,

    /// Log level (debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: LogLevel,

    /// Request timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LevelFilter {
    fn from(val: LogLevel) -> Self {
        match val {
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Error => LevelFilter::ERROR,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize logging with the specified level
    fmt().with_max_level(args.log_level).init();

    println!("Hodei Console Web SSR Server");
    println!("==============================");

    // Parse bind address
    let addr: SocketAddr = args
        .bind
        .parse()
        .with_context(|| format!("Invalid bind address: {}", args.bind))?;

    info!("Bind address: {}", addr);
    info!("gRPC address: {}", args.grpc_address);
    info!("gRPC proxy enabled: {}", args.enable_grpc_proxy);
    info!("Request timeout: {}s", args.timeout);

    // Start the server
    server::run_with_config(addr, args.grpc_address, args.enable_grpc_proxy)
        .await
        .context("Failed to start server")?;

    Ok(())
}
