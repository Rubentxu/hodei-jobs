//! Workers page - Monitor and manage worker nodes

use crate::components::{
    IconVariant, StatsCard, StatusBadge, WorkerDetailPanel, WorkerDetailStatus,
};
use crate::server_functions::{
    ProviderType, WorkerDisplayStatus, WorkerInfo, get_fallback_workers,
};
use leptos::prelude::*;

#[component]
pub fn Workers() -> impl IntoView {
    let workers = RwSignal::new(get_fallback_workers());

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Workers"</h1>
                    <p class="page-subtitle">{format!("{} worker nodes", workers.get().len())}</p>
                </div>
            </div>

            <div class="stats-grid">
                <StatsCard label="Total Workers".to_string() value={workers.get().len().to_string()} icon="layers".to_string() icon_variant=IconVariant::Primary />
                <StatsCard label="Online".to_string() value={workers.get().iter().filter(|w| matches!(w.status, WorkerDisplayStatus::Online)).count().to_string()} icon="check_circle".to_string() icon_variant=IconVariant::Success />
                <StatsCard label="Busy".to_string() value={workers.get().iter().filter(|w| matches!(w.status, WorkerDisplayStatus::Busy)).count().to_string()} icon="bolt".to_string() icon_variant=IconVariant::Primary />
                <StatsCard label="Offline".to_string() value={workers.get().iter().filter(|w| matches!(w.status, WorkerDisplayStatus::Offline | WorkerDisplayStatus::Error)).count().to_string()} icon="warning".to_string() icon_variant=IconVariant::Danger />
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
                            {move || workers.get().into_iter().map(|worker| {
                                let status = match worker.status {
                                    WorkerDisplayStatus::Online => crate::components::JobStatusBadge::Success,
                                    WorkerDisplayStatus::Busy => crate::components::JobStatusBadge::Running,
                                    WorkerDisplayStatus::Offline | WorkerDisplayStatus::Error => crate::components::JobStatusBadge::Failed,
                                };
                                let status_text = match worker.status {
                                    WorkerDisplayStatus::Online => "Online",
                                    WorkerDisplayStatus::Busy => "Busy",
                                    WorkerDisplayStatus::Offline => "Offline",
                                    WorkerDisplayStatus::Error => "Error",
                                };
                                view! {
                                    <tr>
                                        <td>
                                            <StatusBadge
                                                status=status
                                                label=status_text.to_string()
                                                animate={matches!(worker.status, WorkerDisplayStatus::Busy)}
                                            />
                                        </td>
                                        <td>
                                            <span style="font-weight: 500;">{worker.name}</span>
                                            <span class="monospace" style="display: block; font-size: 0.75rem; color: var(--text-muted);">{worker.id}</span>
                                        </td>
                                        <td>{worker.provider}</td>
                                        <td>
                                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                                <div style="width: 60px; height: 6px; background: var(--bg-tertiary); border-radius: 3px; overflow: hidden;">
                                                    <div style=format!("height: 100%; width: {}%; background: var(--color-success);", worker.cpu_usage)></div>
                                                </div>
                                                <span style="font-size: 0.875rem;">{format!("{}%", worker.cpu_usage)}</span>
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
