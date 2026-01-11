//! Providers page - Configure and manage worker providers

use crate::components::provider_forms::{ProviderFormState, ProviderType};
use crate::components::provider_wizard::{ProviderWizard, WizardState};
use crate::server_functions::{
    ProviderDisplayStatus, ProviderType as ServerProviderType, get_fallback_providers,
};
use leptos::prelude::*;

/// Providers page component
#[component]
pub fn Providers() -> impl IntoView {
    let providers = RwSignal::new(get_fallback_providers());
    let wizard_state = RwSignal::new(WizardState::default());

    let open_wizard = move |ptype: Option<ProviderType>| {
        wizard_state.update(|s| {
            s.is_open = true;
            s.provider_type = ptype.clone();
            s.form_state.provider_type = ptype;
            s.current_step = if s.provider_type.is_some() { 1 } else { 0 };
        });
    };

    let on_submit = Callback::new(move |data: ProviderFormState| {
        leptos::logging::log!("Submitted provider data: {:?}", data);
    });

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Providers"</h1>
                    <p class="page-subtitle">"Configure worker infrastructure providers"</p>
                </div>
                <div class="quick-actions">
                    <button class="btn btn-primary" on:click=move |_| open_wizard(None)>
                        <span class="material-symbols-outlined">"add"</span>
                        "Add Provider"
                    </button>
                </div>
            </div>

            // Provider Cards Grid
            <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 1.5rem; margin-bottom: 2rem;">
                {move || providers.get().into_iter().map(|provider| {
                    let icon = match provider.provider_type {
                        ServerProviderType::Kubernetes => "container",
                        ServerProviderType::Docker => "docker",
                        ServerProviderType::Firecracker => "security",
                    };
                    let color = match provider.provider_type {
                        ServerProviderType::Kubernetes => "var(--color-info)",
                        ServerProviderType::Docker => "var(--color-primary)",
                        ServerProviderType::Firecracker => "var(--color-warning)",
                    };
                    let status_badge = match provider.status {
                        ProviderDisplayStatus::Connected => ("Connected".to_string(), "badge-success".to_string()),
                        ProviderDisplayStatus::Disconnected => ("Disconnected".to_string(), "badge-danger".to_string()),
                        ProviderDisplayStatus::Error => ("Error".to_string(), "badge-warning".to_string()),
                    };
                    let provider_type_label = match provider.provider_type {
                        ServerProviderType::Kubernetes => "Kubernetes Cluster",
                        ServerProviderType::Docker => "Docker Daemon",
                        ServerProviderType::Firecracker => "Firecracker MicroVMs",
                    };

                    view! {
                        <div class="card card-hover">
                            <div class="card-body">
                                <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 1rem;">
                                    <div style="display: flex; align-items: center; gap: 1rem;">
                                        <div style=format!("width: 48px; height: 48px; border-radius: 12px; background: {}; display: flex; align-items: center; justify-content: center;", color)>
                                            <span class="material-symbols-outlined" style="font-size: 1.5rem; color: white;">{icon}</span>
                                        </div>
                                        <div>
                                            <h3 style="font-size: 1.125rem; font-weight: 600; margin: 0;">{provider.name}</h3>
                                            <p style="font-size: 0.875rem; color: var(--text-secondary); margin: 0.25rem 0 0;">
                                                {provider_type_label}
                                            </p>
                                        </div>
                                    </div>
                                    <span class={status_badge.1}>{status_badge.0}</span>
                                </div>

                                <p style="font-size: 0.875rem; color: var(--text-secondary); margin-bottom: 1rem;">
                                    {provider.description}
                                </p>

                                <div style="display: flex; gap: 0.5rem;">
                                    <button class="btn btn-primary btn-sm">
                                        <span class="material-symbols-outlined" style="font-size: 1rem;">"settings"</span>
                                        "Configure"
                                    </button>
                                    <button class="btn btn-secondary btn-sm">
                                        <span class="material-symbols-outlined" style="font-size: 1rem;">"health_and_safety"</span>
                                        "Test"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>

            // Add Provider Section
            <div class="card">
                <div class="card-header">
                    <h3 class="card-title">"Add New Provider"</h3>
                </div>
                <div class="card-body">
                    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem;">
                        <div class="card" style="text-align: center; cursor: pointer;" on:click=move |_| open_wizard(Some(ProviderType::Kubernetes))>
                            <div class="card-body" style="padding: 2rem;">
                                <div style="width: 64px; height: 64px; border-radius: 16px; background: var(--color-info); display: flex; align-items: center; justify-content: center; margin: 0 auto 1rem;">
                                    <span class="material-symbols-outlined" style="font-size: 2rem; color: white;">"container"</span>
                                </div>
                                <h4 style="font-size: 1rem; font-weight: 600; margin-bottom: 0.5rem;">"Kubernetes"</h4>
                                <p style="font-size: 0.875rem; color: var(--text-secondary); margin-bottom: 1rem;">
                                    "Deploy workers on Kubernetes clusters"
                                </p>
                                <button class="btn btn-secondary btn-sm">"Add"</button>
                            </div>
                        </div>
                        <div class="card" style="text-align: center; cursor: pointer;" on:click=move |_| open_wizard(Some(ProviderType::Docker))>
                            <div class="card-body" style="padding: 2rem;">
                                <div style="width: 64px; height: 64px; border-radius: 16px; background: var(--color-primary); display: flex; align-items: center; justify-content: center; margin: 0 auto 1rem;">
                                    <span class="material-symbols-outlined" style="font-size: 2rem; color: white;">"docker"</span>
                                </div>
                                <h4 style="font-size: 1rem; font-weight: 600; margin-bottom: 0.5rem;">"Docker"</h4>
                                <p style="font-size: 0.875rem; color: var(--text-secondary); margin-bottom: 1rem;">
                                    "Run workers as Docker containers"
                                </p>
                                <button class="btn btn-secondary btn-sm">"Add"</button>
                            </div>
                        </div>
                        <div class="card" style="text-align: center; cursor: pointer;" on:click=move |_| open_wizard(Some(ProviderType::Firecracker))>
                            <div class="card-body" style="padding: 2rem;">
                                <div style="width: 64px; height: 64px; border-radius: 16px; background: var(--color-warning); display: flex; align-items: center; justify-content: center; margin: 0 auto 1rem;">
                                    <span class="material-symbols-outlined" style="font-size: 2rem; color: white;">"security"</span>
                                </div>
                                <h4 style="font-size: 1rem; font-weight: 600; margin-bottom: 0.5rem;">"Firecracker"</h4>
                                <p style="font-size: 0.875rem; color: var(--text-secondary); margin-bottom: 1rem;">
                                    "Launch lightweight microVMs"
                                </p>
                                <button class="btn btn-secondary btn-sm">"Add"</button>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <ProviderWizard state=wizard_state on_submit=on_submit />
        </div>
    }
}
