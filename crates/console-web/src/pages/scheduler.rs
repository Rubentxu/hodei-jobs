//! Scheduler & Queue Management Page
//!
//! Visualizes the current state of the job scheduler, queue status, and allows configuration.

use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct QueueItem {
    id: String,
    name: String,
    priority: String,
    submitted_by: String,
    status: String,
    wait_time: String,
}

#[component]
pub fn Scheduler() -> impl IntoView {
    // Sample data
    let queue_items = vec![
        QueueItem {
            id: "job-123".to_string(),
            name: "Data Pipeline A".to_string(),
            priority: "High".to_string(),
            submitted_by: "alice".to_string(),
            status: "Queued".to_string(),
            wait_time: "00:02:15".to_string(),
        },
        QueueItem {
            id: "job-124".to_string(),
            name: "Report Gen".to_string(),
            priority: "Medium".to_string(),
            submitted_by: "bob".to_string(),
            status: "Scheduling".to_string(),
            wait_time: "00:00:45".to_string(),
        },
        QueueItem {
            id: "job-125".to_string(),
            name: "Batch Process".to_string(),
            priority: "Low".to_string(),
            submitted_by: "system".to_string(),
            status: "Pending".to_string(),
            wait_time: "00:00:10".to_string(),
        },
    ];

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Scheduler & Queue"</h1>
                    <p class="page-subtitle">"Manage job scheduling, queue prioritization, and resource allocation"</p>
                </div>
                <div class="last-updated">
                    "Last updated: Just now"
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
                                <span class="kpi-value">"42"</span>
                                <span class="kpi-trend positive">"+12% vs last hour"</span>
                            </div>
                        </div>
                        <div class="card kpi-card">
                            <div class="kpi-icon blue">
                                <span class="material-symbols-outlined">"pending_actions"</span>
                            </div>
                            <div class="kpi-data">
                                <span class="kpi-label">"Active Scheduling"</span>
                                <span class="kpi-value">"8"</span>
                                <div class="progress-bar-mini">
                                    <div class="progress-fill" style="width: 75%"></div>
                                </div>
                            </div>
                        </div>
                        <div class="card kpi-card">
                            <div class="kpi-icon indigo">
                                <span class="material-symbols-outlined">"timer"</span>
                            </div>
                            <div class="kpi-data">
                                <span class="kpi-label">"Avg. Wait Time"</span>
                                <span class="kpi-value">"14s"</span>
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
                                    {queue_items.into_iter().map(|item| {
                                        let priority_val = item.priority.clone();
                                        let priority_match = item.priority.clone();
                                        let priority_icon = match priority_match.as_str() {
                                            "High" => "priority_high",
                                            "Low" => "arrow_downward",
                                            _ => "remove",
                                        };
                                        let is_high = priority_match == "High";
                                        let is_low = priority_match == "Low";
                                        let submitted_by = item.submitted_by.clone();
                                        let submitted_by_text = item.submitted_by.clone();

                                        view! {
                                            <tr>
                                                <td>
                                                    <div class="priority-cell" title=priority_val>
                                                        <span class="material-symbols-outlined" class:text-danger=is_high class:text-warning={!is_high && !is_low} class:text-sub=is_low>
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
                                                    <span class="badge badge-outline">{item.status}</span>
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
                                <span class="material-symbols-outlined text-warning">"warning"</span>
                                <div class="health-info">
                                    <span class="health-label">"Worker Availability"</span>
                                    <span class="health-value">"92% online"</span>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
