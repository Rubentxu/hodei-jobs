//! Jobs page - List and manage job executions

use crate::components::StatusBadge;
use crate::server_functions::{JobSummaryStatus, RecentJob, get_fallback_recent_jobs};
use leptos::prelude::*;

/// Jobs page component
#[component]
pub fn Jobs() -> impl IntoView {
    let jobs = RwSignal::new(get_fallback_recent_jobs());
    let search_term = RwSignal::new(String::new());
    let status_filter = RwSignal::new(String::from("all"));
    let selected_count = RwSignal::new(0i32);

    let filtered_jobs = Signal::derive(move || {
        let search = search_term.get().to_lowercase();
        let status = status_filter.get();
        jobs.get()
            .into_iter()
            .filter(|j| {
                let search_match = search.is_empty()
                    || j.name.to_lowercase().contains(&search)
                    || j.id.to_lowercase().contains(&search);
                let status_match = status == "all"
                    || match j.status {
                        JobSummaryStatus::Running => status == "running",
                        JobSummaryStatus::Success => status == "success",
                        JobSummaryStatus::Failed => status == "failed",
                        JobSummaryStatus::Pending => status == "pending",
                    };
                search_match && status_match
            })
            .collect::<Vec<RecentJob>>()
    });

    let has_selection = move || selected_count.get() > 0;

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Jobs"</h1>
                    <p class="page-subtitle">
                        {move || format!("{} total jobs", jobs.get().len())}
                    </p>
                </div>
                <div class="quick-actions">
                    <button class="btn btn-primary">
                        <span class="material-symbols-outlined">"add"</span>
                        "New Job"
                    </button>
                </div>
            </div>

            <div class="bulk-actions-bar" style={move || if has_selection() { "display: flex;" } else { "display: none;" }}>
                <div class="bulk-actions-content">
                    <span class="bulk-actions-count">
                        {move || format!("{} selected", selected_count.get())}
                    </span>
                    <div class="bulk-actions-buttons">
                        <button class="btn btn-sm btn-warning">
                            <span class="material-symbols-outlined">"cancel"</span>
                            "Cancel"
                        </button>
                        <button class="btn btn-sm btn-danger">
                            <span class="material-symbols-outlined">"delete"</span>
                            "Delete"
                        </button>
                    </div>
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
                                <select
                                    class="form-select"
                                    on:change=move |e| {
                                        status_filter.set(event_target_value(&e));
                                    }
                                >
                                    <option value="all">"All"</option>
                                    <option value="running">"Running"</option>
                                    <option value="success">"Success"</option>
                                    <option value="failed">"Failed"</option>
                                    <option value="pending">"Pending"</option>
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
                                    <th style="width: 40px;">
                                        <input type="checkbox" class="form-checkbox" />
                                    </th>
                                    <th>"Status"</th>
                                    <th>"Job Name"</th>
                                    <th>"Duration"</th>
                                    <th>"Started"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    let list = filtered_jobs.get();
                                    list.into_iter().map(|job| {
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
                                                    <input type="checkbox" class="form-checkbox" />
                                                </td>
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
