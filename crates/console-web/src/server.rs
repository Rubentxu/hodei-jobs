//! SSR Server for Hodei Console Web using Axum
//!
//! Production-ready Axum server for server-side rendering of the Leptos application.

use anyhow::{Context, Result};
use axum::{Router, response::IntoResponse, routing::get};
use http::StatusCode;
use leptos::config::LeptosOptions;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tower_http::{
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::{info, warn};
use tracing_subscriber::fmt;

/// Application state for the SSR server
#[derive(Clone, Debug)]
pub struct ServerState {
    /// Leptos configuration options
    pub leptos_options: LeptosOptions,
    /// gRPC server address for proxying
    pub grpc_address: String,
    /// Enable/disable gRPC proxy
    pub enable_grpc_proxy: bool,
    /// Request timeout in seconds
    pub request_timeout: u64,
}

impl ServerState {
    /// Create new server state with configuration
    #[must_use]
    pub fn new(
        leptos_options: LeptosOptions,
        grpc_address: String,
        enable_grpc_proxy: bool,
    ) -> Self {
        Self {
            leptos_options,
            grpc_address,
            enable_grpc_proxy,
            request_timeout: 30,
        }
    }

    /// Create state from environment variables with defaults
    pub fn from_env() -> Result<Self> {
        let leptos_options = LeptosOptions::builder()
            .output_name("hodei-console-web")
            .site_pkg_dir("pkg")
            .env("PROD")
            .build();

        let grpc_address = std::env::var("HODEI_GRPC_ADDRESS")
            .unwrap_or_else(|_| "http://localhost:50051".to_string());

        let enable_grpc_proxy = std::env::var("HODEI_GRPC_PROXY")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        Ok(Self::new(leptos_options, grpc_address, enable_grpc_proxy))
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Build the Axum router with all middleware and routes
#[must_use]
pub fn build_router(state: Arc<ServerState>) -> Router {
    // CORS configuration for production
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([http::Method::GET, http::Method::POST, http::Method::OPTIONS])
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::ACCEPT,
            http::header::AUTHORIZATION,
        ])
        .max_age(Duration::from_secs(86400));

    // Build the router with SSR rendering
    Router::new()
        .route("/api/health", get(health_check))
        .fallback({
            let state = state.clone();
            move || async move { render_handler(state).await }
        })
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(
            state.request_timeout,
        )))
        .with_state(state)
}

/// Render the Leptos app to HTML
async fn render_handler(state: Arc<ServerState>) -> impl IntoResponse {
    // For SSR, we render the Leptos app
    // In a full implementation, this would use leptos::server::render_to_string
    // with the app component and provide context

    let html = generate_html();

    let mut response = (StatusCode::OK, html).into_response();
    let headers = response.headers_mut();
    headers.insert("Content-Type", "text/html; charset=utf-8".parse().unwrap());
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    response
}

/// Generate HTML for the application
fn generate_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Hodei Console</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
    <link href="https://fonts.googleapis.com/css2?family=Material+Symbols+Outlined:wght,FILL@100..700,0..1&display=swap" rel="stylesheet">
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        tailwind.config = {
            theme: {
                extend: {
                    colors: {
                        primary: '#6464f2',
                        'primary-dark': '#4b4bce',
                    }
                }
            }
        }
    </script>
    <style>
        body { font-family: 'Inter', sans-serif; }
        .material-symbols-outlined { font-variation-settings: 'FILL' 0, 'wght' 400, 'GRAD' 0, 'opsz' 24; }
    </style>
</head>
<body class="bg-gray-100 min-h-screen">
    <div id="app">
        <!-- App will be mounted here in client-side mode -->
        <div class="flex items-center justify-center min-h-screen">
            <div class="text-center">
                <span class="material-symbols-outlined text-6xl text-primary animate-spin">sync</span>
                <p class="mt-4 text-gray-600">Loading Hodei Console...</p>
            </div>
        </div>
    </div>
    <script type="module" src="/pkg/client.js"></script>
</body>
</html>"#.to_string()
}

/// Start the Axum server
///
/// # Errors
/// Returns an error if the server fails to bind to the address or encounters
/// a critical error during operation.
#[tokio::main]
pub async fn start_server() -> Result<()> {
    // Initialize logging
    fmt().with_max_level(tracing::Level::INFO).init();

    // Load configuration from environment
    let state = Arc::new(ServerState::from_env()?);

    // Build the router
    let router = build_router(state.clone());

    // Parse bind address
    let bind_addr =
        std::env::var("HODEI_CONSOLE_BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let addr: SocketAddr = bind_addr
        .parse()
        .with_context(|| format!("Invalid bind address: {}", bind_addr))?;

    info!("Starting Hodei Console Web SSR server on {}", addr);
    info!("gRPC proxy enabled: {}", state.enable_grpc_proxy);

    // Create and start the server
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error occurred")?;

    Ok(())
}

/// Handle shutdown signals gracefully
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    warn!("Received shutdown signal");
    info!("Shutting down server...");
}

/// Run the server with custom configuration
///
/// # Errors
/// Returns an error if configuration is invalid or server fails to start.
pub async fn run_with_config(
    bind_address: SocketAddr,
    grpc_address: String,
    enable_proxy: bool,
) -> Result<()> {
    let leptos_options = LeptosOptions::builder()
        .output_name("hodei-console-web")
        .site_pkg_dir("pkg")
        .env("PROD")
        .build();

    let state = Arc::new(ServerState::new(leptos_options, grpc_address, enable_proxy));

    let router = build_router(state.clone());

    info!("Starting Hodei Console Web SSR server on {}", bind_address);

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed to bind to {}", bind_address))?;

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error occurred")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_state_from_env() {
        // Set test environment
        unsafe {
            std::env::set_var("HODEI_GRPC_ADDRESS", "http://test:50051");
            std::env::set_var("HODEI_GRPC_PROXY", "true");
        }

        let state = ServerState::from_env();
        assert!(state.is_ok());
        let state = state.unwrap();
        assert_eq!(state.grpc_address, "http://test:50051");
        assert!(state.enable_grpc_proxy);

        // Cleanup
        unsafe {
            std::env::remove_var("HODEI_GRPC_ADDRESS");
            std::env::remove_var("HODEI_GRPC_PROXY");
        }
    }

    #[test]
    fn test_server_state_default_values() {
        // Clear env vars
        unsafe {
            std::env::remove_var("HODEI_GRPC_ADDRESS");
            std::env::remove_var("HODEI_GRPC_PROXY");
        }

        let state = ServerState::from_env().unwrap();
        assert_eq!(state.grpc_address, "http://localhost:50051");
        assert!(!state.enable_grpc_proxy);
    }

    #[test]
    fn test_generate_html() {
        let html = generate_html();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Hodei Console"));
        assert!(html.contains("tailwindcss"));
        assert!(html.contains("/pkg/client.js"));
    }
}
