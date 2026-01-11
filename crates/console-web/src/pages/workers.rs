//! Workers page - Monitor and manage worker nodes

use crate::components::{
    IconVariant, StatsCard, StatusBadge, WorkerDetailPanel, WorkerDetailStatus,
};
use leptos::prelude::*;

#[derive(Clone, Debug)]
struct Worker {
    id: String,
    name: String,
    status: WorkerDetailStatus,
    cpu: i32,
    provider: String,
}

#[component]
pub fn Workers() -> impl IntoView {
    let workers = vec![
        Worker {
            id: "worker-001".to_string(),
            name: "k8s-worker-1".to_string(),
            status: WorkerDetailStatus::Busy,
            cpu: 72,
            provider: "Kubernetes".to_string(),
        },
        Worker {
            id: "worker-002".to_string(),
            name: "k8s-worker-2".to_string(),
            status: WorkerDetailStatus::Online,
            cpu: 23,
            provider: "Kubernetes".to_string(),
        },
        Worker {
            id: "worker-003".to_string(),
            name: "docker-worker-1".to_string(),
            status: WorkerDetailStatus::Online,
            cpu: 45,
            provider: "Docker".to_string(),
        },
        Worker {
            id: "worker-004".to_string(),
            name: "fc-worker-1".to_string(),
            status: WorkerDetailStatus::Offline,
            cpu: 0,
            provider: "Firecracker".to_string(),
        },
    ];

    let total = workers.len() as i32;
    let online = workers
        .iter()
        .filter(|w| w.status == WorkerDetailStatus::Online)
        .count() as i32;
    let busy = workers
        .iter()
        .filter(|w| w.status == WorkerDetailStatus::Busy)
        .count() as i32;

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Workers"</h1>
                    <p class="page-subtitle">{format!("{} worker nodes", total)}</p>
                </div>
            </div>

            <div class="stats-grid">
                <StatsCard label="Total Workers".to_string() value=total.to_string() icon="layers".to_string() icon_variant=IconVariant::Primary />
                <StatsCard label="Online".to_string() value=online.to_string() icon="check_circle".to_string() icon_variant=IconVariant::Success />
                <StatsCard label="Busy".to_string() value=busy.to_string() icon="bolt".to_string() icon_variant=IconVariant::Primary />
                <StatsCard label="Offline".to_string() value={(total - online - busy).to_string()} icon="warning".to_string() icon_variant=IconVariant::Danger />
            </div>

            <div class="card">
                <div class="card-body" style="padding: 0;">
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Status"</th>
                                <th>"Worker"</th>
                                <th>"Provider"</th>
                                <th>"CPU"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {workers.into_iter().map(|worker| {
                                let status = match worker.status {
                                    WorkerDetailStatus::Online => crate::components::JobStatusBadge::Success,
                                    WorkerDetailStatus::Busy => crate::components::JobStatusBadge::Running,
                                    WorkerDetailStatus::Offline | WorkerDetailStatus::Error => crate::components::JobStatusBadge::Failed,
                                };
                                let status_text = match worker.status {
                                    WorkerDetailStatus::Online => "Online",
                                    WorkerDetailStatus::Busy => "Busy",
                                    WorkerDetailStatus::Offline => "Offline",
                                    WorkerDetailStatus::Error => "Error",
                                };
                                view! {
                                    <tr>
                                        <td><StatusBadge status=status label=status_text.to_string() animate={matches!(worker.status, WorkerDetailStatus::Busy)} /></td>
                                        <td>
                                            <span style="font-weight: 500;">{worker.name}</span>
                                            <span class="monospace" style="display: block; font-size: 0.75rem; color: var(--text-muted);">{worker.id}</span>
                                        </td>
                                        <td>{worker.provider}</td>
                                        <td>
                                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                                <div style="width: 60px; height: 6px; background: var(--bg-tertiary); border-radius: 3px; overflow: hidden;">
                                                    <div style=format!("height: 100%; width: {}%; background: var(--color-success);", worker.cpu)></div>
                                                </div>
                                                <span style="font-size: 0.875rem;">{format!("{}%", worker.cpu)}</span>
                                            </div>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                </div>
            </div>

            <WorkerDetailPanel worker=None on_close=Action::new(|_| async {}) />
        </div>
    }
}
