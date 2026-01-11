//! Jobs page - List and manage job executions

use crate::components::StatusBadge;
use crate::server_functions::{JobSummaryStatus, RecentJob, get_fallback_recent_jobs};
use leptos::prelude::*;

/// Jobs page component
#[component]
pub fn Jobs() -> impl IntoView {
    // State for jobs
    let jobs = RwSignal::new(get_fallback_recent_jobs());
    let search_term = RwSignal::new(String::new());
    let total_count = RwSignal::new(3i32);

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Jobs"</h1>
                    <p class="page-subtitle">
                        {format!("{} total jobs", total_count.get())}
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
                                <input
                                    type="text"
                                    class="form-input"
                                    placeholder="Search jobs..."
                                    on:input=move |e| {
                                        search_term.set(event_target_value(&e));
                                    }
                                />
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
                                {move || {
                                    let term = search_term.get().to_lowercase();
                                    let all_jobs = jobs.get();
                                    let filtered: Vec<RecentJob> = if term.is_empty() {
                                        all_jobs
                                    } else {
                                        all_jobs
                                            .into_iter()
                                            .filter(|j| {
                                                j.name.to_lowercase().contains(&term) || j.id.to_lowercase().contains(&term)
                                            })
                                            .collect()
                                    };
                                    filtered.into_iter().map(|job| {
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
                                    }).collect::<Vec<_>>()
                                }}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    }
}
