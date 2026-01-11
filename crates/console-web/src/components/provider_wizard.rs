//! Provider Wizard Forms - Configure infrastructure providers
//!
//! Provides wizard dialogs for adding Kubernetes, Docker, and Firecracker providers.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Provider types
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ProviderType {
    #[serde(rename = "kubernetes")]
    Kubernetes,
    #[serde(rename = "docker")]
    Docker,
    #[serde(rename = "firecracker")]
    Firecracker,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Kubernetes => write!(f, "Kubernetes"),
            ProviderType::Docker => write!(f, "Docker"),
            ProviderType::Firecracker => write!(f, "Firecracker"),
        }
    }
}

/// Wizard state
#[derive(Clone, Debug, Default)]
pub struct WizardState {
    pub is_open: bool,
    pub current_step: usize,
    pub provider_type: Option<ProviderType>,
    pub form_data: ProviderFormData,
}

/// Provider form data
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProviderFormData {
    pub name: String,
    pub description: String,
    // Kubernetes fields
    pub kubeconfig: String,
    pub cluster_url: String,
    pub namespace: String,
    // Docker fields
    pub docker_socket: String,
    pub docker_tls: bool,
    pub docker_ca_cert: String,
    pub docker_client_cert: String,
    pub docker_client_key: String,
    // Firecracker fields
    pub fc_socket_path: String,
    pub fc_kernel_image: String,
    pub fc_rootfs: String,
    pub fc_vcpu: u32,
    pub fc_mem_mib: u32,
}

/// Provider wizard component
#[component]
pub fn ProviderWizard(
    state: RwSignal<WizardState>,
    on_submit: Callback<ProviderFormData>,
) -> impl IntoView {
    let close_wizard = move |_| {
        state.set(WizardState::default());
    };

    let next_step = move |_| {
        state.update(|s| {
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
        let data = state.get().form_data.clone();
        on_submit.run(data);
        state.update(|s| s.is_open = false);
    };

    view! {
        <Show when={move || state.get().is_open}>
            <div class="modal-overlay" on:click={close_wizard}>
                <div class="modal-content wizard-modal" on:click=|_| {}>
                    <div class="wizard-header">
                        <h2 class="wizard-title">
                            {move || match state.get().provider_type {
                                Some(ProviderType::Kubernetes) => "Add Kubernetes Provider",
                                Some(ProviderType::Docker) => "Add Docker Provider",
                                Some(ProviderType::Firecracker) => "Add Firecracker Provider",
                                None => "Add Provider",
                            }}
                        </h2>
                        <button class="modal-close" on:click={close_wizard}>"×"</button>
                    </div>

                    <div class="wizard-progress">
                        <div class="progress-step" class:active={move || state.get().current_step >= 0}>
                            <span class="step-number">"1"</span>
                            <span class="step-label">"Basic Info"</span>
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
                            <BasicInfoForm />
                        </Show>
                        <Show when={move || state.get().current_step == 1}>
                            <ConfigForm />
                        </Show>
                        <Show when={move || state.get().current_step == 2}>
                            <ReviewForm />
                        </Show>
                    </div>

                    <div class="wizard-footer">
                        <Show when={move || state.get().current_step > 0}>
                            <button class="btn btn-secondary" on:click={prev_step}>
                                "Previous"
                            </button>
                        </Show>
                        <div class="spacer"></div>
                        <Show when={move || state.get().current_step < 2}>
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

/// Basic info form step
#[component]
fn BasicInfoForm() -> impl IntoView {
    view! {
        <div class="form-step">
            <div class="form-group">
                <label for="provider-name">"Provider Name"</label>
                <input
                    id="provider-name"
                    type="text"
                    class="form-input"
                    placeholder="e.g., Production Kubernetes Cluster"
                />
            </div>
            <div class="form-group">
                <label for="provider-description">"Description"</label>
                <textarea
                    id="provider-description"
                    class="form-textarea"
                    placeholder="Brief description of this provider..."
                ></textarea>
            </div>
        </div>
    }
}

/// Configuration form step
#[component]
fn ConfigForm() -> impl IntoView {
    view! {
        <div class="form-step">
            <Show when={move || true /* placeholder for provider_type check */}>
                <KubernetesConfigForm />
            </Show>
        </div>
    }
}

/// Kubernetes configuration form
#[component]
fn KubernetesConfigForm() -> impl IntoView {
    view! {
        <div class="provider-config">
            <div class="config-section">
                <h4>"Kubernetes Configuration"</h4>
                <div class="form-group">
                    <label for="kubeconfig">"Kubeconfig (Base64 encoded)"</label>
                    <textarea
                        id="kubeconfig"
                        class="form-textarea code"
                        placeholder="Paste your kubeconfig content here..."
                        rows="8"
                    ></textarea>
                    <p class="form-help">"Base64 encoded kubeconfig content. Get it with: cat ~/.kube/config | base64 -w0"</p>
                </div>
                <div class="form-row">
                    <div class="form-group">
                        <label for="cluster-url">"Cluster URL (optional)"</label>
                        <input
                            id="cluster-url"
                            type="text"
                            class="form-input"
                            placeholder="https://kubernetes.example.com:6443"
                        />
                    </div>
                    <div class="form-group">
                        <label for="namespace">"Default Namespace"</label>
                        <input
                            id="namespace"
                            type="text"
                            class="form-input"
                            placeholder="hodei-workers"
                        />
                    </div>
                </div>
            </div>
        </div>
    }
}

/// Review form step
#[component]
fn ReviewForm() -> impl IntoView {
    view! {
        <div class="form-step">
            <div class="review-section">
                <h4>"Review Configuration"</h4>
                <p class="form-help">"Review your provider configuration before adding."</p>
            </div>
        </div>
    }
}

/// Provider type selector component
#[component]
pub fn ProviderTypeSelector(on_select: Callback<ProviderType>) -> impl IntoView {
    let on_kubernetes = move |_| {
        on_select.run(ProviderType::Kubernetes);
    };
    let on_docker = move |_| {
        on_select.run(ProviderType::Docker);
    };
    let on_firecracker = move |_| {
        on_select.run(ProviderType::Firecracker);
    };

    view! {
        <div class="provider-type-grid">
            <button class="provider-type-card" on:click={on_kubernetes}>
                <div class="type-icon kubernetes">
                    <span class="material-symbols-outlined">"container"</span>
                </div>
                <h4>"Kubernetes"</h4>
                <p>"Deploy workers on Kubernetes clusters with full orchestration"</p>
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
                <p>"Launch ultra-lightweight microVMs for maximum performance"</p>
            </button>
        </div>
    }
}
