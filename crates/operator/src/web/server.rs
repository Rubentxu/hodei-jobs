//! HTTP Server for Dashboard using Axum
//!
//! Serves the Leptos WASM dashboard and API endpoints.

use axum::{
    Json, Router,
    response::Html,
    routing::{get, get_service},
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tower::ServiceExt;

/// Dashboard statistics response
#[derive(Serialize)]
pub struct DashboardStats {
    pub total_jobs: u32,
    pub running_jobs: u32,
    pub completed_jobs: u32,
    pub failed_jobs: u32,
    pub total_worker_pools: u32,
    pub total_provider_configs: u32,
    pub timestamp: String,
}

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

/// Start the HTTP server for the dashboard
#[cfg(feature = "dashboard")]
pub async fn start_server(
    port: u16,
    static_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let static_path = Arc::new(static_path);

    // API routes
    let api = Router::new()
        .route(
            "/v1/dashboard/stats",
            get(|| async {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                Json(DashboardStats {
                    total_jobs: 0,
                    running_jobs: 0,
                    completed_jobs: 0,
                    failed_jobs: 0,
                    total_worker_pools: 0,
                    total_provider_configs: 0,
                    timestamp: format!("{}", now),
                })
            }),
        )
        .route(
            "/health",
            get(|| async {
                Json(HealthResponse {
                    status: "healthy".to_string(),
                    service: "hodei-console-web".to_string(),
                })
            }),
        );

    // Static files service
    let static_files = get_service(tower_http::services::ServeDir::new(&*static_path))
        .handle_error(|error| async move {
            Ok::<_, std::convert::Infallible>(
                axum::response::Response::builder()
                    .status(404)
                    .body(axum::body::Body::empty())?,
            )
        });

    // SPA fallback - serve index.html for non-file routes
    let spa_fallback = get(|| async {
        let index_path = static_path.join("index.html");
        if index_path.exists() {
            let content = tokio::fs::read_to_string(index_path)
                .await
                .unwrap_or_default();
            Html(content)
        } else {
            Html("<html><body><h1>Hodei Operator Dashboard</h1><p>Dashboard files not found.</p></body></html>".to_string())
        }
    });

    // Combine routes
    let app = Router::new()
        .route("/api/*path", get(api.clone()).post(api.clone()))
        .route("/health", get(api))
        .route("/pkg/*path", get(static_files.clone()))
        .route("/style.css", get(static_files.clone()))
        .fallback(spa_fallback);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    println!("Dashboard server starting on http://{}", addr);
    println!("Static files from: {:?}", static_path);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    Ok(())
}

/// Serve dashboard from assets directory
#[cfg(feature = "dashboard")]
pub async fn serve_dashboard(port: u16, assets_path: Option<PathBuf>) {
    let path =
        assets_path.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));

    if let Err(e) = start_server(port, path).await {
        eprintln!("Dashboard server error: {}", e);
    }
}
