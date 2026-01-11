//! REST API endpoints for the Dashboard UI
//! Provides HTTP endpoints that complement the gRPC API

use axum::{
    Json, Router,
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Types (simplified versions matching dashboard expectations)
// ============================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DashboardStats {
    pub total_jobs: u32,
    pub running_jobs: u32,
    pub pending_jobs: u32,
    pub succeeded_jobs: u32,
    pub failed_jobs: u32,
    pub cancelled_jobs: u32,
    pub total_pools: u32,
    pub total_providers: u32,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub command: String,
    pub arguments: Vec<String>,
    pub status: String,
    pub provider: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkerPool {
    pub name: String,
    pub namespace: String,
    pub provider: String,
    pub image: String,
    pub current_replicas: i32,
    pub min_replicas: i32,
    pub max_replicas: i32,
    pub resources: PoolResources,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PoolResources {
    pub requests: Option<ResourceLimits>,
    pub limits: Option<ResourceLimits>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResourceLimits {
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Provider {
    pub name: String,
    pub provider_type: String,
    pub config: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OperatorSettings {
    pub namespace: String,
    pub watch_namespace: String,
    pub server_address: String,
    pub default_timeout: i32,
}

// ============================================================================
// API State
// ============================================================================

pub struct ApiState {
    // These would be connected to actual services in production
    // For now, we return mock data that matches the dashboard expectations
}

impl ApiState {
    pub fn new() -> Self {
        Self {}
    }
}

// ============================================================================
// Routes
// ============================================================================

pub fn create_router() -> Router<Arc<ApiState>> {
    let state = Arc::new(ApiState::new());

    Router::new()
        .route("/api/v1/dashboard/stats", get(get_dashboard_stats))
        .route("/api/v1/jobs", get(get_jobs))
        .route("/api/v1/jobs/:id", get(get_job))
        .route("/api/v1/jobs/:id/cancel", post(cancel_job))
        .route("/api/v1/pools", get(get_pools))
        .route("/api/v1/providers", get(get_providers))
        .route("/api/v1/settings", get(get_settings))
        .route("/api/v1/settings", put(update_settings))
        .route("/health", get(health_check))
        .with_state(state)
}

// ============================================================================
// Handlers
// ============================================================================

async fn health_check() -> &'static str {
    "OK"
}

async fn get_dashboard_stats() -> Json<DashboardStats> {
    Json(DashboardStats {
        total_jobs: 0,
        running_jobs: 0,
        pending_jobs: 0,
        succeeded_jobs: 0,
        failed_jobs: 0,
        cancelled_jobs: 0,
        total_pools: 0,
        total_providers: 0,
        timestamp: Utc::now().to_rfc3339(),
    })
}

async fn get_jobs() -> Json<Vec<Job>> {
    Json(vec![])
}

async fn get_job(path: axum::extract::Path<String>) -> Json<Option<Job>> {
    let _id = path.0;
    Json(None)
}

async fn cancel_job(path: axum::extract::Path<String>) -> Json<serde_json::Value> {
    let _id = path.0;
    Json(serde_json::json!({
        "success": true,
        "message": "Job cancellation requested"
    }))
}

async fn get_pools() -> Json<Vec<WorkerPool>> {
    Json(vec![])
}

async fn get_providers() -> Json<Vec<Provider>> {
    Json(vec![])
}

async fn get_settings() -> Json<OperatorSettings> {
    Json(OperatorSettings {
        namespace: "hodei-system".to_string(),
        watch_namespace: "".to_string(),
        server_address: "http://localhost:50051".to_string(),
        default_timeout: 3600,
    })
}

async fn update_settings(Json(settings): Json<OperatorSettings>) -> Json<OperatorSettings> {
    settings
}
