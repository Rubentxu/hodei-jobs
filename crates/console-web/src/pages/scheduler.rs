//! Scheduler & Queue Management Page
//!
//! Visualizes the current state of the job scheduler, queue status, and allows configuration.

use crate::server_functions::{Priority, QueueStatus, QueueStatusInfo, get_fallback_queue_items};
use leptos::prelude::*;

#[component]
pub fn Scheduler() -> impl IntoView {
    let queue_status = RwSignal::new(QueueStatusInfo::default());
    let queue_items = RwSignal::new(get_fallback_queue_items());

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Scheduler & Queue"</h1>
                    <p class="page-subtitle">"Manage job scheduling, queue prioritization, and resource allocation"</p>
                </div>
                <div class="last-updated">
                    "Updated just now"
                </div>
            </div>

            <div class="scheduler-grid">
                // Main Content Area
                <div class="main-content">
                    // KPI Cards
                    <div class="kpi-grid">
                        <div class="card kpi-card">
                            <div class="kpi-icon primary">
                                <span class="material-symbols-outlined">"layers"</span>
                            </div>
                            <div class="kpi-data">
                                <span class="kpi-label">"Total Queued"</span>
                                <span class="kpi-value">{queue_status.get().total_queued.to_string()}</span>
                                <span class="kpi-trend positive">"+12% vs last hour"</span>
                            </div>
                        </div>
                        <div class="card kpi-card">
                            <div class="kpi-icon blue">
                                <span class="material-symbols-outlined">"pending_actions"</span>
                            </div>
                            <div class="kpi-data">
                                <span class="kpi-label">"Active Scheduling"</span>
                                <span class="kpi-value">{queue_status.get().active_scheduling.to_string()}</span>
                                <div class="progress-bar-mini">
                                    <div class="progress-fill" style=format!("width: {}%", (queue_status.get().active_scheduling as f64 / 10.0 * 100.0).min(100.0))></div>
                                </div>
                            </div>
                        </div>
                        <div class="card kpi-card">
                            <div class="kpi-icon indigo">
                                <span class="material-symbols-outlined">"timer"</span>
                            </div>
                            <div class="kpi-data">
                                <span class="kpi-label">"Avg. Wait Time"</span>
                                <span class="kpi-value">{format!("{:.1}s", queue_status.get().avg_wait_time_seconds)}</span>
                                <span class="kpi-badge success">"SLA OK"</span>
                            </div>
                        </div>
                    </div>

                    // Queue Table
                    <div class="card queue-table-card">
                        <div class="card-header">
                            <h3 class="card-title">"Live Job Queue"</h3>
                            <div class="card-actions">
                                <button class="btn btn-ghost btn-sm">
                                    <span class="material-symbols-outlined">"filter_list"</span>
                                    "Filter"
                                </button>
                                <button class="btn btn-ghost btn-sm">
                                    <span class="material-symbols-outlined">"refresh"</span>
                                </button>
                            </div>
                        </div>
                        <div class="table-container">
                            <table class="data-table">
                                <thead>
                                    <tr>
                                        <th>"Priority"</th>
                                        <th>"Job ID"</th>
                                        <th>"User"</th>
                                        <th>"Status"</th>
                                        <th>"Wait Time"</th>
                                        <th>"Actions"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || queue_items.get().into_iter().map(|item| {
                                        let priority_match = item.priority.clone();
                                        let priority_icon = match priority_match {
                                            Priority::High => "priority_high",
                                            Priority::Low => "arrow_downward",
                                            Priority::Medium => "remove",
                                        };
                                        let is_high = matches!(priority_match, Priority::High);
                                        let is_low = matches!(priority_match, Priority::Low);
                                        let submitted_by = item.submitted_by.clone();
                                        let submitted_by_text = item.submitted_by.clone();

                                        let status_text = match item.status {
                                            QueueStatus::Queued => "Queued",
                                            QueueStatus::Scheduling => "Scheduling",
                                            QueueStatus::Pending => "Pending",
                                            QueueStatus::Running => "Running",
                                        };

                                        view! {
                                            <tr>
                                                <td>
                                                    <div class="priority-cell" title=format!("{:?}", priority_match)>
                                                        <span class="material-symbols-outlined" class:text-danger=is_high class:text-warning={!is_high && !matches!(priority_match, Priority::Low)} class:text-sub=is_low>
                                                            {priority_icon}
                                                        </span>
                                                    </div>
                                                </td>
                                                <td>
                                                    <div class="job-id-cell">
                                                        <span class="job-name">{item.name}</span>
                                                        <span class="job-id-mono">{item.id}</span>
                                                    </div>
                                                </td>
                                                <td>
                                                    <div class="user-cell">
                                                        <div class="user-avatar">{move || submitted_by[0..1].to_uppercase()}</div>
                                                        <span>{submitted_by_text}</span>
                                                    </div>
                                                </td>
                                                <td>
                                                    <span class="badge badge-outline">{status_text}</span>
                                                </td>
                                                <td class="font-mono">{item.wait_time}</td>
                                                <td>
                                                    <button class="btn-icon">
                                                        <span class="material-symbols-outlined">"more_vert"</span>
                                                    </button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    </div>
                </div>

                // Configuration Sidebar
                <div class="config-sidebar">
                    <div class="card config-card">
                        <div class="card-header">
                            <h3 class="card-title">"Configuration"</h3>
                        </div>
                        <div class="card-body">
                            <div class="form-group">
                                <label class="label">"Scheduling Algorithm"</label>
                                <select class="form-select">
                                    <option>"Fair Share"</option>
                                    <option>"FIFO"</option>
                                    <option>"Priority Only"</option>
                                </select>
                            </div>

                            <div class="form-group">
                                <label class="label row-between">
                                    "Preemption"
                                    <input type="checkbox" class="toggle" checked=true />
                                </label>
                                <p class="help-text">"Allow high priority jobs to preempt lower ones"</p>
                            </div>

                            <div class="form-group">
                                <label class="label">"Active Plugins"</label>
                                <div class="tags-container">
                                    <span class="tag">"node-affinity"</span>
                                    <span class="tag">"pod-spread"</span>
                                    <span class="tag">"bin-packing"</span>
                                    <button class="tag-add">+</button>
                                </div>
                            </div>
                        </div>
                    </div>

                    <div class="card health-card">
                        <div class="card-header">
                            <h3 class="card-title">"System Health"</h3>
                        </div>
                        <div class="card-body">
                            <div class="health-item">
                                <span class="material-symbols-outlined text-success">"check_circle"</span>
                                <div class="health-info">
                                    <span class="health-label">"Scheduler Heartbeat"</span>
                                    <span class="health-value">"5ms latency"</span>
                                </div>
                            </div>
                            <div class="health-item">
                                <span class="material-symbols-outlined text-success">"check_circle"</span>
                                <div class="health-info">
                                    <span class="health-label">"Worker Availability"</span>
                                    <span class="health-value">{format!("{:.0}% online", queue_status.get().workers_online_percent)}</span>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
