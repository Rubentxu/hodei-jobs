//! Providers page - Manage worker providers

use leptos::prelude::*;

#[component]
pub fn Providers() -> impl IntoView {
    view! {
        <div class="providers-page">
            <h1>"Providers"</h1>
            <p>"Configure worker providers (Docker, Kubernetes, Firecracker)"</p>
            <div class="providers-list">
                <div class="provider-card">
                    <h3>"Docker"</h3>
                    <button>"Configure"</button>
                </div>
                <div class="provider-card">
                    <h3>"Kubernetes"</h3>
                    <button>"Configure"</button>
                </div>
                <div class="provider-card">
                    <h3>"Firecracker"</h3>
                    <button>"Configure"</button>
                </div>
            </div>
        </div>
    }
}
