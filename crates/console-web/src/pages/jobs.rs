//! Jobs page - List and manage job executions

use crate::components::StatusBadge;
use leptos::prelude::*;

/// Recent job for display
#[derive(Clone, Debug)]
pub struct RecentJob {
    pub id: String,
    pub name: String,
    pub status: JobSummaryStatus,
    pub duration: String,
    pub started: String,
}

#[derive(Clone, Debug)]
pub enum JobSummaryStatus {
    Running,
    Success,
    Failed,
    Pending,
}

/// Jobs page component
#[component]
pub fn Jobs() -> impl IntoView {
    let jobs = generate_sample_jobs();
    let total_jobs = jobs.len();

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Jobs"</h1>
                    <p class="page-subtitle">
                        {format!("{} total jobs", total_jobs)}
                    </p>
                </div>
                <div class="quick-actions">
                    <button class="btn btn-primary">
                        <span class="material-symbols-outlined">"add"</span>
                        "New Job"
                    </button>
                </div>
            </div>

            <div style="display: grid; grid-template-columns: 280px 1fr; gap: 1.5rem;">
                <aside class="filters-sidebar">
                    <div class="card">
                        <div class="card-header">
                            <h3 class="card-title">"Filters"</h3>
                        </div>
                        <div class="card-body">
                            <div class="form-group">
                                <label class="form-label">"Search"</label>
                                <input type="text" class="form-input" placeholder="Search..." />
                            </div>
                            <div class="form-group">
                                <label class="form-label">"Status"</label>
                                <select class="form-select">
                                    <option value="all">"All"</option>
                                    <option value="running">"Running"</option>
                                    <option value="success">"Success"</option>
                                    <option value="failed">"Failed"</option>
                                </select>
                            </div>
                        </div>
                    </div>
                </aside>

                <div class="card">
                    <div class="card-body" style="padding: 0;">
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Status"</th>
                                    <th>"Job Name"</th>
                                    <th>"Duration"</th>
                                    <th>"Started"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {jobs
                                    .into_iter()
                                    .map(|job| {
                                        let status = match job.status {
                                            JobSummaryStatus::Running => crate::components::JobStatusBadge::Running,
                                            JobSummaryStatus::Success => crate::components::JobStatusBadge::Success,
                                            JobSummaryStatus::Failed => crate::components::JobStatusBadge::Failed,
                                            JobSummaryStatus::Pending => crate::components::JobStatusBadge::Pending,
                                        };
                                        let animate = matches!(job.status, JobSummaryStatus::Running);
                                        view! {
                                            <tr class="clickable">
                                                <td>
                                                    <StatusBadge
                                                        status=status
                                                        label=job.id.clone()
                                                        animate=animate
                                                    />
                                                </td>
                                                <td>
                                                    <span style="font-weight: 500;">{job.name}</span>
                                                    <span class="monospace" style="display: block; font-size: 0.75rem; color: var(--text-muted);">
                                                        {job.id}
                                                    </span>
                                                </td>
                                                <td class="monospace">{job.duration}</td>
                                                <td>{job.started}</td>
                                            </tr>
                                        }
                                    })
                                    .collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    }
}

fn generate_sample_jobs() -> Vec<RecentJob> {
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
            name: "ml-model-training-v2".to_string(),
            status: JobSummaryStatus::Running,
            duration: "14m 22s".to_string(),
            started: "2024-01-11 10:23".to_string(),
        },
        RecentJob {
            id: "job-1245".to_string(),
            name: "image-processing-batch".to_string(),
            status: JobSummaryStatus::Success,
            duration: "45s".to_string(),
            started: "2024-01-11 09:45".to_string(),
        },
    ]
}
