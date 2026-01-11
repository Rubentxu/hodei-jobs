//! Provider Wizard Forms - Configure infrastructure providers
//!
//! Provides wizard dialogs for adding Kubernetes, Docker, and Firecracker providers.

use crate::components::provider_forms::{ProviderFormState, ProviderType};
use leptos::prelude::*;

/// Wizard state
#[derive(Clone, Debug, Default)]
pub struct WizardState {
    pub is_open: bool,
    pub current_step: usize,
    pub provider_type: Option<ProviderType>,
    // Form data is now managed via the robust ProviderFormState
    pub form_state: ProviderFormState,
}

/// Provider wizard component
#[component]
pub fn ProviderWizard(
    state: RwSignal<WizardState>,
    on_submit: Callback<ProviderFormState>,
) -> impl IntoView {
    let close_wizard = move |_| {
        state.set(WizardState::default());
    };

    let next_step = move |_| {
        state.update(|s| {
            // Validate before proceeding from config step
            if s.current_step == 1 {
                if let Err(errors) = s.form_state.validate() {
                    // In a real app we'd show these errors.
                    // For now, we just auto-advance if it's "mostly" okay or let validation
                    // be handled within the form components themselves.
                    // For this implementation, we assume the form components update the state.
                    // and we check valid status.
                    if !errors.is_empty() {
                        return;
                    }
                }
            }

            if s.current_step < 2 {
                s.current_step += 1;
            }
        });
    };

    let prev_step = move |_| {
        state.update(|s| {
            if s.current_step > 0 {
                s.current_step -= 1;
            }
        });
    };

    let handle_submit = move |_| {
        let form_state = state.get().form_state.clone();
        on_submit.run(form_state);
        state.update(|s| s.is_open = false);
    };

    let on_type_select = move |ptype: ProviderType| {
        state.update(|s| {
            s.provider_type = Some(ptype.clone());
            s.form_state.provider_type = Some(ptype);
            s.current_step = 1; // Move to config immediately after selection
        });
    };

    view! {
        <Show when={move || state.get().is_open}>
            <div class="modal-overlay" on:click={close_wizard}>
                <div class="modal-content wizard-modal" on:click=|_| {}> // Stop propagation
                    <div class="wizard-header">
                        <h2 class="wizard-title">
                            {move || match state.get().provider_type {
                                Some(pt) => format!("Add {} Provider", pt.display_name()),
                                None => "Add Provider".to_string(),
                            }}
                        </h2>
                        <button class="modal-close" on:click={close_wizard}>"×"</button>
                    </div>

                    <div class="wizard-progress">
                        <div class="progress-step" class:active={move || state.get().current_step >= 0}>
                            <span class="step-number">"1"</span>
                            <span class="step-label">"Provider Type"</span>
                        </div>
                        <div class="progress-line"></div>
                        <div class="progress-step" class:active={move || state.get().current_step >= 1}>
                            <span class="step-number">"2"</span>
                            <span class="step-label">"Configuration"</span>
                        </div>
                        <div class="progress-line"></div>
                        <div class="progress-step" class:active={move || state.get().current_step >= 2}>
                            <span class="step-number">"3"</span>
                            <span class="step-label">"Review"</span>
                        </div>
                    </div>

                    <div class="wizard-body">
                        <Show when={move || state.get().current_step == 0}>
                            <ProviderTypeSelector on_select=Callback::new(on_type_select) />
                        </Show>
                        <Show when={move || state.get().current_step == 1}>
                            <ConfigForm state=state />
                        </Show>
                        <Show when={move || state.get().current_step == 2}>
                            <ReviewForm state=state />
                        </Show>
                    </div>

                    <div class="wizard-footer">
                        <Show when={move || state.get().current_step > 0}>
                            <button class="btn btn-secondary" on:click={prev_step}>
                                "Previous"
                            </button>
                        </Show>
                        <div class="spacer"></div>
                        <Show when={move || state.get().current_step == 1}>
                             <button class="btn btn-primary" on:click={next_step}>
                                 "Next"
                             </button>
                        </Show>
                        <Show when={move || state.get().current_step == 2}>
                            <button class="btn btn-success" on:click={handle_submit}>
                                "Add Provider"
                            </button>
                        </Show>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Provider type selector component
#[component]
pub fn ProviderTypeSelector(on_select: Callback<ProviderType>) -> impl IntoView {
    let on_kubernetes = move |_| on_select.run(ProviderType::Kubernetes);
    let on_docker = move |_| on_select.run(ProviderType::Docker);
    let on_firecracker = move |_| on_select.run(ProviderType::Firecracker);

    view! {
        <div class="provider-type-grid">
            <button class="provider-type-card" on:click={on_kubernetes}>
                <div class="type-icon kubernetes">
                    <span class="material-symbols-outlined">"container"</span>
                </div>
                <h4>"Kubernetes"</h4>
                <p>"Deploy workers on Kubernetes clusters"</p>
            </button>
            <button class="provider-type-card" on:click={on_docker}>
                <div class="type-icon docker">
                    <span class="material-symbols-outlined">"docker"</span>
                </div>
                <h4>"Docker"</h4>
                <p>"Run workers as lightweight Docker containers"</p>
            </button>
            <button class="provider-type-card" on:click={on_firecracker}>
                <div class="type-icon firecracker">
                    <span class="material-symbols-outlined">"security"</span>
                </div>
                <h4>"Firecracker"</h4>
                <p>"Launch ultra-lightweight microVMs"</p>
            </button>
        </div>
    }
}

/// Configuration form container
#[component]
fn ConfigForm(state: RwSignal<WizardState>) -> impl IntoView {
    view! {
        <div class="form-step">
            <Show when={move || state.get().provider_type == Some(ProviderType::Kubernetes)}>
                <KubernetesConfigForm state=state />
            </Show>
            <Show when={move || state.get().provider_type == Some(ProviderType::Docker)}>
                <DockerConfigForm state=state />
            </Show>
            <Show when={move || state.get().provider_type == Some(ProviderType::Firecracker)}>
                <FirecrackerConfigForm state=state />
            </Show>
            <Show when={move || state.get().provider_type.is_none()}>
                <div>"Please select a provider type"</div>
            </Show>
        </div>
    }
}

/// Kubernetes configuration form
#[component]
fn KubernetesConfigForm(state: RwSignal<WizardState>) -> impl IntoView {
    view! {
        <div class="provider-config">
            <div class="form-group">
                <label>"Name"</label>
                <input type="text" class="form-input"
                    on:input=move |ev| state.update(|s| s.form_state.kubernetes.name = event_target_value(&ev))
                    prop:value=move || state.get().form_state.kubernetes.name
                />
            </div>
             <div class="form-group">
                <label>"Kubeconfig (Base64)"</label>
                <textarea class="form-textarea code"
                    on:input=move |ev| state.update(|s| s.form_state.kubernetes.kubeconfig = event_target_value(&ev))
                    prop:value=move || state.get().form_state.kubernetes.kubeconfig
                ></textarea>
            </div>
            <div class="form-group">
                <label>"Namespace"</label>
                <input type="text" class="form-input"
                    on:input=move |ev| state.update(|s| s.form_state.kubernetes.namespace = event_target_value(&ev))
                    prop:value=move || state.get().form_state.kubernetes.namespace
                />
            </div>
        </div>
    }
}

/// Docker configuration form
#[component]
fn DockerConfigForm(state: RwSignal<WizardState>) -> impl IntoView {
    view! {
        <div class="provider-config">
           <div class="form-group">
                <label>"Name"</label>
                <input type="text" class="form-input"
                    on:input=move |ev| state.update(|s| s.form_state.docker.name = event_target_value(&ev))
                     prop:value=move || state.get().form_state.docker.name
                />
            </div>
            <div class="form-group">
                <label>"Docker Socket"</label>
                <input type="text" class="form-input"
                    placeholder="unix:///var/run/docker.sock"
                    on:input=move |ev| state.update(|s| s.form_state.docker.socket_path = event_target_value(&ev))
                    prop:value=move || state.get().form_state.docker.socket_path
                />
            </div>
        </div>
    }
}

/// Firecracker configuration form
#[component]
fn FirecrackerConfigForm(state: RwSignal<WizardState>) -> impl IntoView {
    view! {
        <div class="provider-config">
           <div class="form-group">
                <label>"Name"</label>
                <input type="text" class="form-input"
                    on:input=move |ev| state.update(|s| s.form_state.firecracker.name = event_target_value(&ev))
                    prop:value=move || state.get().form_state.firecracker.name
                />
            </div>
            <div class="form-group">
                <label>"Socket Path"</label>
                <input type="text" class="form-input"
                    on:input=move |ev| state.update(|s| s.form_state.firecracker.socket_path = event_target_value(&ev))
                    prop:value=move || state.get().form_state.firecracker.socket_path
                />
            </div>
            <div class="form-row">
                 <div class="form-group">
                    <label>"Kernel Image"</label>
                    <input type="text" class="form-input"
                        on:input=move |ev| state.update(|s| s.form_state.firecracker.kernel_image = event_target_value(&ev))
                        prop:value=move || state.get().form_state.firecracker.kernel_image
                    />
                </div>
                 <div class="form-group">
                    <label>"Rootfs"</label>
                    <input type="text" class="form-input"
                        on:input=move |ev| state.update(|s| s.form_state.firecracker.rootfs = event_target_value(&ev))
                        prop:value=move || state.get().form_state.firecracker.rootfs
                    />
                </div>
            </div>
        </div>
    }
}

/// Review form step
#[component]
fn ReviewForm(state: RwSignal<WizardState>) -> impl IntoView {
    view! {
        <div class="form-step">
            <div class="review-section">
                <h4>"Review Configuration"</h4>
                <Show when={move || state.get().provider_type == Some(ProviderType::Kubernetes)}>
                    <div class="review-item">
                        <span class="label">"Type:"</span> <span class="value">"Kubernetes"</span>
                        <span class="label">"Name:"</span> <span class="value">{state.get().form_state.kubernetes.name}</span>
                        <span class="label">"Namespace:"</span> <span class="value">{state.get().form_state.kubernetes.namespace}</span>
                    </div>
                </Show>
                <Show when={move || state.get().provider_type == Some(ProviderType::Docker)}>
                    <div class="review-item">
                        <span class="label">"Type:"</span> <span class="value">"Docker"</span>
                        <span class="label">"Name:"</span> <span class="value">{state.get().form_state.docker.name}</span>
                        <span class="label">"Socket:"</span> <span class="value">{state.get().form_state.docker.socket_path}</span>
                    </div>
                </Show>
                <Show when={move || state.get().provider_type == Some(ProviderType::Firecracker)}>
                    <div class="review-item">
                        <span class="label">"Type:"</span> <span class="value">"Firecracker"</span>
                        <span class="label">"Name:"</span> <span class="value">{state.get().form_state.firecracker.name}</span>
                        <span class="label">"Kernel:"</span> <span class="value">{state.get().form_state.firecracker.kernel_image}</span>
                    </div>
                </Show>
            </div>
        </div>
    }
}
