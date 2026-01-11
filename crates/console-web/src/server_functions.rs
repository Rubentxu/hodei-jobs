//! Server Functions for Hodei Jobs Console
//!
//! Provides data types and fallback data for console pages.
//! Real gRPC integration happens at runtime when connected.
//!
//! This module provides:
//! - Data transfer types for UI components
//! - Mock/fallback data when gRPC server is unavailable
//! - Type definitions matching gRPC responses

use serde::{Deserialize, Serialize};

/// Dashboard statistics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_jobs: u32,
    pub running_jobs: u32,
    pub success_jobs_7d: u32,
    pub failed_jobs_7d: u32,
    pub total_workers: u32,
    pub online_workers: u32,
    pub busy_workers: u32,
    pub queued_jobs: u32,
}

/// Recent job for display
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecentJob {
    pub id: String,
    pub name: String,
    pub status: JobSummaryStatus,
    pub duration: String,
    pub started: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum JobSummaryStatus {
    Running,
    Success,
    Failed,
    Pending,
}

/// Worker information for display
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkerInfo {
    pub id: String,
    pub name: String,
    pub status: WorkerDisplayStatus,
    pub cpu_usage: i32,
    pub provider: String,
    pub provider_type: ProviderType,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum WorkerDisplayStatus {
    Online,
    Busy,
    Offline,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ProviderType {
    Kubernetes,
    Docker,
    Firecracker,
}

/// Provider information for display
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub status: ProviderDisplayStatus,
    pub description: String,
    pub cluster_info: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ProviderDisplayStatus {
    Connected,
    Disconnected,
    Error,
}

/// Queue item for scheduler page
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QueueItemInfo {
    pub id: String,
    pub name: String,
    pub priority: Priority,
    pub submitted_by: String,
    pub status: QueueStatus,
    pub wait_time: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum QueueStatus {
    Queued,
    Scheduling,
    Pending,
    Running,
}

/// Scheduler queue status
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QueueStatusInfo {
    pub total_queued: u32,
    pub active_scheduling: u32,
    pub avg_wait_time_seconds: f64,
    pub workers_online_percent: f64,
}

/// Provider health status for Worker Health Panel
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub provider_name: String,
    pub provider_type: ProviderType,
    pub status: ProviderDisplayStatus,
    pub latency_ms: u32,
    pub worker_count: u32,
    pub last_heartbeat: String,
}

/// Get fallback provider health data
pub fn get_fallback_provider_health() -> Vec<ProviderHealth> {
    vec![
        ProviderHealth {
            provider_id: "provider-k8s-001".to_string(),
            provider_name: "Production Kubernetes".to_string(),
            provider_type: ProviderType::Kubernetes,
            status: ProviderDisplayStatus::Connected,
            latency_ms: 45,
            worker_count: 2,
            last_heartbeat: "2024-01-11 10:35".to_string(),
        },
        ProviderHealth {
            provider_id: "provider-docker-001".to_string(),
            provider_name: "Local Docker".to_string(),
            provider_type: ProviderType::Docker,
            status: ProviderDisplayStatus::Connected,
            latency_ms: 12,
            worker_count: 1,
            last_heartbeat: "2024-01-11 10:35".to_string(),
        },
        ProviderHealth {
            provider_id: "provider-fc-001".to_string(),
            provider_name: "Firecracker Pool".to_string(),
            provider_type: ProviderType::Firecracker,
            status: ProviderDisplayStatus::Disconnected,
            latency_ms: 0,
            worker_count: 0,
            last_heartbeat: "2024-01-11 09:15".to_string(),
        },
    ]
}

/// Default dashboard stats (fallback when gRPC unavailable)
impl Default for DashboardStats {
    fn default() -> Self {
        Self {
            total_jobs: 1247,
            running_jobs: 12,
            success_jobs_7d: 1198,
            failed_jobs_7d: 37,
            total_workers: 4,
            online_workers: 3,
            busy_workers: 1,
            queued_jobs: 5,
        }
    }
}

/// Default queue status (fallback when gRPC unavailable)
impl Default for QueueStatusInfo {
    fn default() -> Self {
        Self {
            total_queued: 42,
            active_scheduling: 8,
            avg_wait_time_seconds: 14.0,
            workers_online_percent: 92.0,
        }
    }
}

/// Get fallback queue status
pub fn get_fallback_queue_status() -> QueueStatusInfo {
    QueueStatusInfo::default()
}

/// Get fallback dashboard stats
pub fn get_fallback_dashboard_stats() -> DashboardStats {
    DashboardStats::default()
}

/// Get fallback recent jobs
pub fn get_fallback_recent_jobs() -> Vec<RecentJob> {
    vec![
        RecentJob {
            id: "job-1247".to_string(),
            name: "data-processing-pipeline".to_string(),
            status: JobSummaryStatus::Success,
            duration: "2m 34s".to_string(),
            started: "2024-01-11 10:30".to_string(),
        },
        RecentJob {
            id: "job-1246".to_string(),
            name: "ml-model-training".to_string(),
            status: JobSummaryStatus::Running,
            duration: "14m 22s".to_string(),
            started: "2024-01-11 10:23".to_string(),
        },
        RecentJob {
            id: "job-1245".to_string(),
            name: "image-processing".to_string(),
            status: JobSummaryStatus::Success,
            duration: "45s".to_string(),
            started: "2024-01-11 09:45".to_string(),
        },
    ]
}

/// Get fallback workers list
pub fn get_fallback_workers() -> Vec<WorkerInfo> {
    vec![
        WorkerInfo {
            id: "worker-001".to_string(),
            name: "k8s-worker-1".to_string(),
            status: WorkerDisplayStatus::Busy,
            cpu_usage: 72,
            provider: "Kubernetes".to_string(),
            provider_type: ProviderType::Kubernetes,
        },
        WorkerInfo {
            id: "worker-002".to_string(),
            name: "k8s-worker-2".to_string(),
            status: WorkerDisplayStatus::Online,
            cpu_usage: 23,
            provider: "Kubernetes".to_string(),
            provider_type: ProviderType::Kubernetes,
        },
        WorkerInfo {
            id: "worker-003".to_string(),
            name: "docker-worker-1".to_string(),
            status: WorkerDisplayStatus::Online,
            cpu_usage: 45,
            provider: "Docker".to_string(),
            provider_type: ProviderType::Docker,
        },
        WorkerInfo {
            id: "worker-004".to_string(),
            name: "fc-worker-1".to_string(),
            status: WorkerDisplayStatus::Offline,
            cpu_usage: 0,
            provider: "Firecracker".to_string(),
            provider_type: ProviderType::Firecracker,
        },
    ]
}

/// Get fallback providers list
pub fn get_fallback_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "provider-k8s-001".to_string(),
            name: "Production Kubernetes".to_string(),
            provider_type: ProviderType::Kubernetes,
            status: ProviderDisplayStatus::Connected,
            description: "Main production cluster in us-east-1 region".to_string(),
            cluster_info: Some("production-cluster.elb.us-east-1.amazonaws.com".to_string()),
        },
        ProviderInfo {
            id: "provider-docker-001".to_string(),
            name: "Local Docker".to_string(),
            provider_type: ProviderType::Docker,
            status: ProviderDisplayStatus::Connected,
            description: "Development Docker daemon".to_string(),
            cluster_info: Some("unix:///var/run/docker.sock".to_string()),
        },
        ProviderInfo {
            id: "provider-fc-001".to_string(),
            name: "Firecracker Pool".to_string(),
            provider_type: ProviderType::Firecracker,
            status: ProviderDisplayStatus::Disconnected,
            description: "High-performance microVM pool".to_string(),
            cluster_info: None,
        },
    ]
}

/// Get fallback queue items
pub fn get_fallback_queue_items() -> Vec<QueueItemInfo> {
    vec![
        QueueItemInfo {
            id: "job-123".to_string(),
            name: "Data Pipeline A".to_string(),
            priority: Priority::High,
            submitted_by: "alice".to_string(),
            status: QueueStatus::Queued,
            wait_time: "00:02:15".to_string(),
        },
        QueueItemInfo {
            id: "job-124".to_string(),
            name: "Report Gen".to_string(),
            priority: Priority::Medium,
            submitted_by: "bob".to_string(),
            status: QueueStatus::Scheduling,
            wait_time: "00:00:45".to_string(),
        },
        QueueItemInfo {
            id: "job-125".to_string(),
            name: "Batch Process".to_string(),
            priority: Priority::Low,
            submitted_by: "system".to_string(),
            status: QueueStatus::Pending,
            wait_time: "00:00:10".to_string(),
        },
    ]
}
