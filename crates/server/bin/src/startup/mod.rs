//! Startup module - orchestrates application initialization.
//!
//! Uses the existing MigrationService from infrastructure for migrations.

use hodei_server_infrastructure::persistence::postgres::migrations::{
    MigrationConfig, run_migrations,
};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::time::Duration;
use tracing::info;

/// Current application version
pub const APP_VERSION: &str = "0.38.5";

/// Application state containing all initialized components.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool
    pub pool: sqlx::PgPool,
}

impl AppState {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

/// Configuration for application startup.
#[derive(Debug, Clone)]
pub struct StartupConfig {
    pub grpc_addr: SocketAddr,
    pub db_pool_size: u32,
    pub db_min_idle: u32,
    pub db_connect_timeout: Duration,
    pub db_idle_timeout: Duration,
    pub db_max_lifetime: Duration,
    pub nats_urls: String,
    pub nats_timeout: Duration,
    pub dev_mode: bool,
    pub provisioning_enabled: bool,
    pub worker_image: String,
    pub docker_enabled: bool,
    pub kubernetes_enabled: bool,
    pub rust_log: String,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "0.0.0.0:9090".parse().unwrap(),
            db_pool_size: 50,
            db_min_idle: 10,
            db_connect_timeout: Duration::from_secs(30),
            db_idle_timeout: Duration::from_secs(600),
            db_max_lifetime: Duration::from_secs(1800),
            nats_urls: "nats://localhost:4222".to_string(),
            nats_timeout: Duration::from_secs(10),
            dev_mode: false,
            provisioning_enabled: true,
            worker_image: "hodei-jobs-worker:latest".to_string(),
            docker_enabled: true,
            kubernetes_enabled: true,
            rust_log: "info".to_string(),
        }
    }
}

impl StartupConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        use std::env;

        let grpc_port: u16 = env::var("GRPC_PORT")
            .unwrap_or_else(|_| "9090".to_string())
            .parse()?;

        let host = env::var("HODEI_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let grpc_addr = format!("{}:{}", host, grpc_port).parse()?;

        Ok(Self {
            grpc_addr,
            nats_urls: env::var("HODEI_NATS_URLS")
                .unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            dev_mode: env::var("HODEI_DEV_MODE").unwrap_or_default() == "1",
            provisioning_enabled: env::var("HODEI_PROVISIONING_ENABLED")
                .unwrap_or_else(|_| "1".to_string())
                == "1",
            worker_image: env::var("HODEI_WORKER_IMAGE")
                .unwrap_or_else(|_| "hodei-jobs-worker:latest".to_string()),
            docker_enabled: env::var("HODEI_DOCKER_ENABLED").unwrap_or_else(|_| "1".to_string())
                == "1",
            kubernetes_enabled: env::var("HODEI_KUBERNETES_ENABLED")
                .unwrap_or_else(|_| "1".to_string())
                == "1",
            rust_log: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            ..Self::default()
        })
    }

    pub fn database_url(&self) -> anyhow::Result<String> {
        std::env::var("SERVER_DATABASE_URL")
            .or_else(|_| std::env::var("HODEI_DATABASE_URL"))
            .or_else(|_| std::env::var("DATABASE_URL"))
            .map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))
    }
}

/// Run the complete application startup sequence.
pub async fn run(config: StartupConfig) -> anyhow::Result<AppState> {
    info!(
        "Starting Hodei Jobs Platform v{} on {}",
        APP_VERSION, config.grpc_addr
    );

    // Step 1: Connect to database
    let pool = connect_to_database(&config).await?;
    info!("✓ Database connected");

    // Step 2: Run migrations using existing infrastructure service
    let config_migration = MigrationConfig::default();
    let validation = run_migrations(&pool).await?;
    info!("✓ Migrations: {}", validation.summary());

    Ok(AppState::new(pool))
}

/// Connect to database.
async fn connect_to_database(config: &StartupConfig) -> anyhow::Result<sqlx::PgPool> {
    let db_url = config.database_url()?;
    info!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(config.db_pool_size)
        .min_connections(config.db_min_idle)
        .acquire_timeout(config.db_connect_timeout)
        .idle_timeout(config.db_idle_timeout)
        .max_lifetime(config.db_max_lifetime)
        .connect(&db_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    Ok(pool)
}
