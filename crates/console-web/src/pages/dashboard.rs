//! Dashboard page - Overview of system status

use crate::components::{IconVariant, StatsCard, StatusBadge};
use crate::server_functions::{
    DashboardStats, JobSummaryStatus, RecentJob, get_fallback_dashboard_stats,
    get_fallback_recent_jobs,
};
use leptos::prelude::*;

/// Dashboard page component
#[component]
pub fn Dashboard() -> impl IntoView {
    // State for dashboard data
    let stats = RwSignal::new(DashboardStats::default());
    let recent_jobs = RwSignal::new(get_fallback_recent_jobs());

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
    }
}
