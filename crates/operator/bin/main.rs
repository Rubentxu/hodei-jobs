//! Hodei Operator - Main Entry Point

use anyhow::{Context, Result};
use axum::ServiceExt;
use axum::{Json, extract::Extension, routing::get};
use clap::{Parser, ValueEnum};
use hodei_operator::OperatorState;
use hodei_operator::grpc::GrpcClient;
use hodei_operator::watcher::{
    create_job_controller, create_provider_config_controller, create_worker_pool_controller,
};
use kube::Client;
use std::net::SocketAddr;
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

    /// Web server port (dashboard)
    #[arg(long, default_value = "8080")]
    pub web_port: u16,

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

    // Start web server (dashboard)
    start_html_server(args.web_port).await?;

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
    info!("Dashboard available at http://localhost:{}", args.web_port);
    let _ = signal::ctrl_c().await;
    info!("Shutting down operator...");

    Ok(())
}

/// Simple HTML dashboard server
async fn start_html_server(port: u16) -> Result<()> {
    use axum::Json;
    use axum::routing::get;

    const LOADING_ROW: &str =
        r#"<tr><td colspan="4" style="text-align:center;color:#666;">Loading...</td></tr>"#;

    let html_template = Arc::new(format!(
        r#"<!DOCTYPE html>
<html lang="es">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Hodei Operator Dashboard</title>
    <style>
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{ font-family: -apple-system, sans-serif; background: #f5f5f5; color: #333; }}
        .navbar {{ background: linear-gradient(135deg, #667eea, #764ba2); color: white; padding: 1rem 2rem; }}
        .title {{ font-size: 1.25rem; font-weight: 600; }}
        main {{ padding: 2rem; max-width: 1200px; margin: 0 auto; }}
        h1 {{ font-size: 1.75rem; margin-bottom: 1.5rem; }}
        .stats-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 1rem; margin-bottom: 2rem; }}
        .stat-card {{ background: white; padding: 1.5rem; border-radius: 10px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .stat-card h3 {{ color: #666; font-size: 0.875rem; margin-bottom: 0.5rem; }}
        .stat-card .value {{ font-size: 2rem; font-weight: 700; }}
        .section {{ background: white; border-radius: 10px; padding: 1.5rem; box-shadow: 0 2px 4px rgba(0,0,0,0.1); margin-bottom: 1.5rem; }}
        .data-table {{ width: 100%; border-collapse: collapse; }}
        .data-table th, .data-table td {{ padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid #e5e7eb; }}
        .data-table th {{ background: #f9fafb; font-weight: 600; color: #666; font-size: 0.75rem; text-transform: uppercase; }}
        .status {{ padding: 0.25rem 0.75rem; border-radius: 9999px; font-size: 0.75rem; background: #d1fae5; color: #065f46; }}
        .pools-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 1rem; }}
        .pool-card {{ background: white; border-radius: 10px; padding: 1.25rem; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .pool-header {{ display: flex; justify-content: space-between; margin-bottom: 1rem; }}
        .provider-badge {{ background: #e5e7eb; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; }}
        .pool-stats {{ display: flex; gap: 1.5rem; }}
        .pool-stat {{ display: flex; flex-direction: column; }}
        .pool-stat .label {{ font-size: 0.75rem; color: #666; }}
        .pool-stat .value {{ font-weight: 600; }}
        .providers-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 1rem; }}
        .provider-card {{ background: white; border-radius: 10px; padding: 1.25rem; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        .enabled {{ background: #d1fae5; color: #065f46; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; }}
    </style>
</head>
<body>
    <nav class="navbar"><span class="title">Hodei Operator</span></nav>
    <main>
        <h1>Dashboard</h1>
        <div class="stats-grid">
            <div class="stat-card"><h3>Total Jobs</h3><div class="value" id="total-jobs">-</div></div>
            <div class="stat-card"><h3>Running</h3><div class="value" id="running-jobs">-</div></div>
            <div class="stat-card"><h3>Completed</h3><div class="value" id="completed-jobs">-</div></div>
            <div class="stat-card"><h3>Failed</h3><div class="value" id="failed-jobs">-</div></div>
        </div>
        <div class="section">
            <h2>Recent Jobs</h2>
            <table class="data-table"><thead><tr><th>Name</th><th>Namespace</th><th>Command</th><th>Status</th></tr></thead>
            <tbody id="jobs-table">{}</tbody></table>
        </div>
        <div class="section"><h2>Worker Pools</h2><div class="pools-grid" id="pools-grid"></div></div>
        <div class="section"><h2>Providers</h2><div class="providers-grid" id="providers-grid"></div></div>
    </main>
    <script>
        async function loadData() {{
            try {{
                const [stats, jobs, pools, provs] = await Promise.all([
                    fetch('/api/stats').then(r => r.json()),
                    fetch('/api/jobs').then(r => r.json()),
                    fetch('/api/worker-pools').then(r => r.json()),
                    fetch('/api/providers').then(r => r.json())
                ]);
                document.getElementById('total-jobs').textContent = stats.total_jobs || 0;
                document.getElementById('running-jobs').textContent = stats.running_jobs || 0;
                document.getElementById('completed-jobs').textContent = stats.completed_jobs || 0;
                document.getElementById('failed-jobs').textContent = stats.failed_jobs || 0;
                document.getElementById('jobs-table').innerHTML = jobs.map(j =>
                    '<tr><td><strong>' + j.name + '</strong></td><td>' + j.namespace + '</td><td><code>' + j.command + '</code></td><td><span class="status">' + j.status + '</span></td></tr>'
                ).join('');
                document.getElementById('pools-grid').innerHTML = pools.map(p =>
                    '<div class="pool-card"><div class="pool-header"><h3>' + p.name + '</h3><span class="provider-badge">' + p.provider + '</span></div><div class="pool-stats"><div class="pool-stat"><span class="label">Min</span><span class="value">' + p.min_replicas + '</span></div><div class="pool-stat"><span class="label">Max</span><span class="value">' + p.max_replicas + '</span></div><div class="pool-stat"><span class="label">Current</span><span class="value">' + p.current_replicas + '</span></div></div></div>'
                ).join('');
                document.getElementById('providers-grid').innerHTML = provs.map(p =>
                    '<div class="provider-card"><div class="pool-header"><h3>' + p.name + '</h3><span class="enabled">' + (p.enabled ? 'Enabled' : 'Disabled') + '</span></div><p style="color:#666;font-size:0.875rem;">Type: ' + p.provider_type + '</p></div>'
                ).join('');
            }} catch(e) {{ console.log('Using static data'); }}
        }}
        loadData();
        setInterval(loadData, 10000);
    </script>
</body>
</html>"#,
        LOADING_ROW
    ));

    async fn html_handler(
        Extension(html): axum::Extension<Arc<String>>,
    ) -> axum::response::Html<String> {
        axum::response::Html(html.to_string())
    }

    let app = axum::Router::new()
        .route("/", get(html_handler))
        .layer(axum::Extension(Arc::clone(&html_template)))
        .route("/api/stats", get(|| async {
            Json(serde_json::json!({
                "total_jobs": 5,
                "running_jobs": 2,
                "completed_jobs": 2,
                "failed_jobs": 1
            }))
        }))
        .route("/api/jobs", get(|| async {
            Json(serde_json::json!([
                {"name": "example-job", "namespace": "hodei-system", "command": "/bin/sh", "status": "Running"}
            ]))
        }))
        .route("/api/worker-pools", get(|| async {
            Json(serde_json::json!([
                {"name": "default-pool", "namespace": "hodei-system", "provider": "kubernetes", "min_replicas": 1, "max_replicas": 5, "current_replicas": 2}
            ]))
        }))
        .route("/api/providers", get(|| async {
            Json(serde_json::json!([
                {"name": "docker-local", "namespace": "hodei-system", "provider_type": "docker", "enabled": true}
            ]))
        }))
        .nest_service("/pkg", tower_http::services::ServeDir::new("/var/www/html"))
        .nest_service("/assets", tower_http::services::ServeDir::new("/var/www/html/assets"));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting dashboard at http://0.0.0.0:{}", port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .await
        .context("Web server failed")?;

    Ok(())
}
