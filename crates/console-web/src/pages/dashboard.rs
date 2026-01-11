//! Dashboard page - Overview of system status

use crate::components::{IconVariant, StatsCard, StatusBadge};
use crate::server_functions::{
    DashboardStats, JobSummaryStatus, ProviderHealth, ProviderType, get_fallback_provider_health,
    get_fallback_recent_jobs,
};
use leptos::prelude::*;

/// Get provider icon based on type
fn get_provider_icon(provider_type: ProviderType) -> &'static str {
    match provider_type {
        ProviderType::Kubernetes => "k8s",
        ProviderType::Docker => "docker",
        ProviderType::Firecracker => "memory",
    }
}

/// Get status class for provider health
fn get_status_class(status: crate::server_functions::ProviderDisplayStatus) -> &'static str {
    match status {
        crate::server_functions::ProviderDisplayStatus::Connected => "status-connected",
        crate::server_functions::ProviderDisplayStatus::Disconnected => "status-disconnected",
        crate::server_functions::ProviderDisplayStatus::Error => "status-error",
    }
}

/// Worker Health Panel Component
#[component]
fn WorkerHealthPanel(providers: Vec<ProviderHealth>) -> impl IntoView {
    view! {
        <div class="worker-health-panel">
            <div class="card-header">
                <h3 class="card-title">
                    <span class="material-symbols-outlined">"hub"</span>
                    "Worker Health"
                </h3>
                <span class="badge badge-info">{providers.len()}" providers"</span>
            </div>
            <div class="card-body">
                <div class="provider-cards">
                    {providers.into_iter().map(|provider| {
                        let status_class = get_status_class(provider.status.clone());
                        let latency_display = if provider.latency_ms > 0 {
                            format!("{}ms", provider.latency_ms)
                        } else {
                            "N/A".to_string()
                        };
                        let provider_type_str = format!("{:?}", provider.provider_type);
                        let status_text = format!("{:?}", provider.status);
                        let provider_type = provider.provider_type;
                        view! {
                            <div class="provider-card" class:connected=matches!(provider.status, crate::server_functions::ProviderDisplayStatus::Connected)>
                                <div class="provider-header">
                                    <div class="provider-icon">
                                        <span class="material-symbols-outlined">{get_provider_icon(provider_type)}</span>
                                    </div>
                                    <div class="provider-info">
                                        <h4 class="provider-name">{provider.provider_name}</h4>
                                        <span class="provider-type">{provider_type_str}</span>
                                    </div>
                                    <div class="provider-status">
                                        <span class=format!("status-dot {}", status_class)></span>
                                        <span class="status-text">{status_text}</span>
                                    </div>
                                </div>
                                <div class="provider-metrics">
                                    <div class="metric">
                                        <span class="metric-label">"Latency"</span>
                                        <span class="metric-value">{latency_display}</span>
                                    </div>
                                    <div class="metric">
                                        <span class="metric-label">"Workers"</span>
                                        <span class="metric-value">{provider.worker_count}</span>
                                    </div>
                                    <div class="metric">
                                        <span class="metric-label">"Last Heartbeat"</span>
                                        <span class="metric-value">{provider.last_heartbeat}</span>
                                    </div>
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </div>
        </div>
    }
}

/// Dashboard page component
#[component]
pub fn Dashboard() -> impl IntoView {
    // State for dashboard data
    let stats = RwSignal::new(DashboardStats::default());
    let recent_jobs = RwSignal::new(get_fallback_recent_jobs());
    let provider_health = RwSignal::new(get_fallback_provider_health());

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Dashboard"</h1>
                    <p class="page-subtitle">"Real-time overview of your Hodei Jobs Platform"</p>
                </div>
                <div class="quick-actions">
                    <button class="btn btn-primary">
                        <span class="material-symbols-outlined">"add"</span>
                        "New Job"
                    </button>
                </div>
            </div>

            // Stats Grid
            <div class="stats-grid">
                <StatsCard
                    label="Total Jobs".to_string()
                    value=stats.get().total_jobs.to_string()
                    icon="work".to_string()
                    icon_variant=IconVariant::Primary
                />
                <StatsCard
                    label="Running".to_string()
                    value=stats.get().running_jobs.to_string()
                    icon="sync".to_string()
                    icon_variant=IconVariant::Primary
                />
                <StatsCard
                    label="Success (7d)".to_string()
                    value=stats.get().success_jobs_7d.to_string()
                    icon="check_circle".to_string()
                    icon_variant=IconVariant::Success
                />
                <StatsCard
                    label="Failed (7d)".to_string()
                    value=stats.get().failed_jobs_7d.to_string()
                    icon="error".to_string()
                    icon_variant=IconVariant::Danger
                />
            </div>

            // Worker Health Panel
            <WorkerHealthPanel providers=provider_health.get() />

            <div class="dashboard-grid">
                // Recent Jobs Table
                <div class="card">
                    <div class="card-header">
                        <h3 class="card-title">"Recent Jobs"</h3>
                    </div>
                    <div class="card-body" style="padding: 0;">
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Status"</th>
                                    <th>"Job ID"</th>
                                    <th>"Name"</th>
                                    <th>"Duration"</th>
                                    <th>"Started"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || recent_jobs.get().into_iter().map(|job| {
                                    let status = match job.status {
                                        JobSummaryStatus::Running => crate::components::JobStatusBadge::Running,
                                        JobSummaryStatus::Success => crate::components::JobStatusBadge::Success,
                                        JobSummaryStatus::Failed => crate::components::JobStatusBadge::Failed,
                                        JobSummaryStatus::Pending => crate::components::JobStatusBadge::Pending,
                                    };
                                    let animate = matches!(job.status, JobSummaryStatus::Running);
                                    view! {
                                        <tr>
                                            <td>
                                                <StatusBadge
                                                    status=status
                                                    label=job.id.clone()
                                                    animate=animate
                                                />
                                            </td>
                                            <td class="monospace">{job.id}</td>
                                            <td>{job.name}</td>
                                            <td class="monospace">{job.duration}</td>
                                            <td>{job.started}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    }
}
