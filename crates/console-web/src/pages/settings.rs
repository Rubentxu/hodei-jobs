//! Settings page - System configuration

use leptos::prelude::*;

#[component]
pub fn Settings() -> impl IntoView {
    view! {
        <div class="settings-page">
            <h1>"Settings"</h1>
            <div class="settings-section">
                <h2>"General"</h2>
                <label>
                    "Default Namespace"
                    <input type="text" value="default" />
                </label>
                <label>
                    "Job Timeout (seconds)"
                    <input type="number" value="3600" />
                </label>
            </div>
            <button>"Save Settings"</button>
        </div>
    }
}
