//! Dashboard page - Overview of system status

use leptos::prelude::*;

#[component]
pub fn Dashboard() -> impl IntoView {
    view! {
        <div class="dashboard">
            <h1>"Hodei Jobs Console"</h1>
            <p>"Administration console for Hodei Jobs Platform"</p>

            <div class="stats-grid">
                <div class="stat-card">
                    <h3>"Total Jobs"</h3>
                    <p class="stat-value">"0"</p>
                </div>
                <div class="stat-card">
                    <h3>"Running"</h3>
                    <p class="stat-value">"0"</p>
                </div>
                <div class="stat-card">
                    <h3>"Completed"</h3>
                    <p class="stat-value">"0"</p>
                </div>
                <div class="stat-card">
                    <h3>"Failed"</h3>
                    <p class="stat-value">"0"</p>
                </div>
                <div class="stat-card">
                    <h3>"Worker Pools"</h3>
                    <p class="stat-value">"0"</p>
                </div>
                <div class="stat-card">
                    <h3>"Providers"</h3>
                    <p class="stat-value">"0"</p>
                </div>
            </div>

            <div class="quick-links">
                <h2>"Quick Links"</h2>
                <ul>
                    <li><a href="/jobs">"Manage Jobs"</a></li>
                    <li><a href="/pools">"Configure Worker Pools"</a></li>
                    <li><a href="/providers">"Manage Providers"</a></li>
                    <li><a href="/settings">"System Settings"</a></li>
                </ul>
            </div>
        </div>
    }
}
