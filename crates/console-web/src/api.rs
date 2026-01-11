//! API client module (for WASM/client mode fallback)
//!
//! This module provides HTTP-based API fallback for when gRPC is not available.
//! In SSR mode with gRPC, this module is not used.

use crate::types::*;
use serde::{Deserialize, Serialize};

/// API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// Dashboard API client
#[derive(Clone)]
pub struct DashboardApi;

impl DashboardApi {
    pub async fn get_stats() -> Result<DashboardStats, String> {
        // Placeholder - would call HTTP endpoint in client mode
        Ok(DashboardStats {
            total_jobs: 0,
            running_jobs: 0,
            completed_jobs: 0,
            failed_jobs: 0,
            total_pools: 0,
            active_pools: 0,
            total_providers: 0,
            enabled_providers: 0,
        })
    }
}

/// Jobs API client
#[derive(Clone)]
pub struct JobsApi;

impl JobsApi {
    pub async fn list() -> Result<Vec<Job>, String> {
        Ok(vec![])
    }

    pub async fn get(id: &str) -> Result<Job, String> {
        Err(format!("Job not found: {}", id))
    }

    pub async fn cancel(id: &str) -> Result<bool, String> {
        Ok(true)
    }
}

/// Providers API client
#[derive(Clone)]
pub struct ProvidersApi;

impl ProvidersApi {
    pub async fn list() -> Result<Vec<Provider>, String> {
        Ok(vec![])
    }

    pub async fn create(input: ProviderInput) -> Result<Provider, String> {
        Err("Not implemented".to_string())
    }

    pub async fn delete(id: &str) -> Result<bool, String> {
        Ok(true)
    }

    pub async fn test(id: &str) -> Result<bool, String> {
        Ok(true)
    }
}

/// Pools API client
#[derive(Clone)]
pub struct PoolsApi;

impl PoolsApi {
    pub async fn list() -> Result<Vec<WorkerPool>, String> {
        Ok(vec![])
    }

    pub async fn create(input: WorkerPoolInput) -> Result<WorkerPool, String> {
        Err("Not implemented".to_string())
    }

    pub async fn delete(id: &str) -> Result<bool, String> {
        Ok(true)
    }
}

/// Settings API client
#[derive(Clone)]
pub struct SettingsApi;

impl SettingsApi {
    pub async fn get() -> Result<OperatorSettings, String> {
        Ok(OperatorSettings {
            namespace: "hodei-system".to_string(),
            watch_namespace: "".to_string(),
            server_address: "hodei-server:50051".to_string(),
            log_level: LogLevel::Info,
            health_check_interval: 30,
        })
    }

    pub async fn save(settings: OperatorSettings) -> Result<bool, String> {
        Ok(true)
    }
}
