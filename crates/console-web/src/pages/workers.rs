//! Workers page - Monitor and manage worker nodes

use crate::components::{IconVariant, StatsCard, StatusBadge, WorkerDetailPanel};
use crate::server_functions::{ProviderType, WorkerDisplayStatus, get_fallback_workers};
use leptos::prelude::*;
use std::collections::HashSet;

#[component]
pub fn Workers() -> impl IntoView {
    let workers = RwSignal::new(get_fallback_workers());

    // Filter signals for UI
    let search_term = RwSignal::new(String::new());
    let selected_statuses: RwSignal<HashSet<WorkerDisplayStatus>> = RwSignal::new(HashSet::new());
    let selected_providers: RwSignal<HashSet<ProviderType>> = RwSignal::new(HashSet::new());

    // Filtered workers derived signal
    let filtered_workers = Signal::derive(move || {
        let search = search_term.get();
        let statuses = selected_statuses.get();
        let providers = selected_providers.get();

        workers
            .get()
            .into_iter()
            .filter(|worker| {
                // Search term filter
                let search_match = if search.is_empty() {
                    true
                } else {
                    let term = search.to_lowercase();
                    worker.name.to_lowercase().contains(&term)
                        || worker.id.to_lowercase().contains(&term)
                };

                // Status filter
                let status_match = statuses.is_empty() || statuses.contains(&worker.status);

                // Provider filter
                let provider_match =
                    providers.is_empty() || providers.contains(&worker.provider_type);

                search_match && status_match && provider_match
            })
            .collect::<Vec<_>>()
    });

    // Stats derived from filtered workers
    let total_count = Signal::derive(move || filtered_workers.get().len());
    let online_count = Signal::derive(move || {
        filtered_workers
            .get()
            .iter()
            .filter(|w| matches!(w.status, WorkerDisplayStatus::Online))
            .count()
    });
    let busy_count = Signal::derive(move || {
        filtered_workers
            .get()
            .iter()
            .filter(|w| matches!(w.status, WorkerDisplayStatus::Busy))
            .count()
    });
    let offline_count = Signal::derive(move || {
        filtered_workers
            .get()
            .iter()
            .filter(|w| {
                matches!(
                    w.status,
                    WorkerDisplayStatus::Offline | WorkerDisplayStatus::Error
                )
            })
            .count()
    });

    // Toggle functions
    let toggle_status = move |status: WorkerDisplayStatus| {
        let mut current = selected_statuses.get();
        if current.contains(&status) {
            current.remove(&status);
        } else {
            current.insert(status);
        }
        selected_statuses.set(current);
    };

    let toggle_provider = move |provider: ProviderType| {
        let mut current = selected_providers.get();
        if current.contains(&provider) {
            current.remove(&provider);
        } else {
            current.insert(provider);
        }
        selected_providers.set(current);
    };

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Workers"</h1>
                    <p class="page-subtitle">{format!("{} worker nodes", total_count.get())}</p>
                </div>
            </div>

            <div class="stats-grid">
                <StatsCard
                    label="Total Workers".to_string()
                    value={total_count.get().to_string()}
                    icon="layers".to_string()
                    icon_variant=IconVariant::Primary
                />
                <StatsCard
                    label="Online".to_string()
                    value={online_count.get().to_string()}
                    icon="check_circle".to_string()
                    icon_variant=IconVariant::Success
                />
                <StatsCard
                    label="Busy".to_string()
                    value={busy_count.get().to_string()}
                    icon="bolt".to_string()
                    icon_variant=IconVariant::Primary
                />
                <StatsCard
                    label="Offline".to_string()
                    value={offline_count.get().to_string()}
                    icon="warning".to_string()
                    icon_variant=IconVariant::Danger
                />
            </div>

            // Filters Bar
            <div class="filters-bar">
                <div class="filter-group">
                    <label class="filter-label">"Search"</label>
                    <input
                        type="text"
                        class="filter-input"
                        placeholder="Search by name or ID..."
                        on:input=move |e| {
                            search_term.set(event_target_value(&e));
                        }
                    />
                </div>

                <div class="filter-group">
                    <label class="filter-label">"Status"</label>
                    <div class="filter-chips">
                        <button
                            class=move || if selected_statuses.get().contains(&WorkerDisplayStatus::Online) { "filter-chip active" } else { "filter-chip" }
                            on:click=move |_| toggle_status(WorkerDisplayStatus::Online)
                        >
                            "Online"
                        </button>
                        <button
                            class=move || if selected_statuses.get().contains(&WorkerDisplayStatus::Busy) { "filter-chip active" } else { "filter-chip" }
                            on:click=move |_| toggle_status(WorkerDisplayStatus::Busy)
                        >
                            "Busy"
                        </button>
                        <button
                            class=move || if selected_statuses.get().contains(&WorkerDisplayStatus::Offline) { "filter-chip active" } else { "filter-chip" }
                            on:click=move |_| toggle_status(WorkerDisplayStatus::Offline)
                        >
                            "Offline"
                        </button>
                        <button
                            class=move || if selected_statuses.get().contains(&WorkerDisplayStatus::Error) { "filter-chip active" } else { "filter-chip" }
                            on:click=move |_| toggle_status(WorkerDisplayStatus::Error)
                        >
                            "Error"
                        </button>
                    </div>
                </div>

                <div class="filter-group">
                    <label class="filter-label">"Provider"</label>
                    <div class="filter-chips">
                        <button
                            class=move || if selected_providers.get().contains(&ProviderType::Kubernetes) { "filter-chip active" } else { "filter-chip" }
                            on:click=move |_| toggle_provider(ProviderType::Kubernetes)
                        >
                            "Kubernetes"
                        </button>
                        <button
                            class=move || if selected_providers.get().contains(&ProviderType::Docker) { "filter-chip active" } else { "filter-chip" }
                            on:click=move |_| toggle_provider(ProviderType::Docker)
                        >
                            "Docker"
                        </button>
                        <button
                            class=move || if selected_providers.get().contains(&ProviderType::Firecracker) { "filter-chip active" } else { "filter-chip" }
                            on:click=move |_| toggle_provider(ProviderType::Firecracker)
                        >
                            "Firecracker"
                        </button>
                    </div>
                </div>
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
                            {move || filtered_workers.get().into_iter().map(|worker| {
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
