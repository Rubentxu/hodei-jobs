//! Routes module for Leptos Router
//!
//! Provides routing configuration for the console web application.

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::components::NavBar;

/// Main app layout with routing
#[component]
pub fn AppRouter() -> impl IntoView {
    view! {
        <Router>
            <div class="app-layout">
                <NavBar />
                <main class="main-content">
                    <Routes fallback=|| view! { "404 - Page not found" }>
                        <Route path=path!("/") view=HomePage />
                        <Route path=path!("/jobs") view=JobsPage />
                        <Route path=path!("/jobs/:id") view=JobDetailPage />
                        <Route path=path!("/pools") view=PoolsPage />
                        <Route path=path!("/providers") view=ProvidersPage />
                        <Route path=path!("/settings") view=SettingsPage />
                    </Routes>
                </main>
            </div>
        </Router>
    }
}

/// Home/Dashboard page
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <div class="dashboard">
            <h1>"Dashboard"</h1>
            <p>"System overview and metrics"</p>
            <DashboardStats />
        </div>
    }
}

/// Dashboard statistics component
#[component]
fn DashboardStats() -> impl IntoView {
    view! {
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
        </div>
    }
}

/// Jobs page
#[component]
fn JobsPage() -> impl IntoView {
    view! {
        <div class="jobs-page">
            <h1>"Jobs"</h1>
            <p>"Manage job executions"</p>
            <JobsList />
        </div>
    }
}

/// Jobs list component
#[component]
fn JobsList() -> impl IntoView {
    view! {
        <table class="jobs-table">
            <thead>
                <tr>
                    <th>"ID"</th>
                    <th>"Name"</th>
                    <th>"Status"</th>
                    <th>"Created"</th>
                    <th>"Actions"</th>
                </tr>
            </thead>
            <tbody>
                <tr>
                    <td>"-"</td>
                    <td>"-"</td>
                    <td>"-"</td>
                    <td>"-"</td>
                    <td>
                        <button>"View"</button>
                    </td>
                </tr>
            </tbody>
        </table>
    }
}

/// Job detail page
#[component]
fn JobDetailPage() -> impl IntoView {
    view! {
        <div class="job-detail">
            <h1>"Job Details"</h1>
            <p>"Job details will be displayed here"</p>
        </div>
    }
}

/// Worker Pools page
#[component]
fn PoolsPage() -> impl IntoView {
    view! {
        <div class="pools-page">
            <h1>"Worker Pools"</h1>
            <p>"Configure worker pool scaling"</p>
            <PoolList />
        </div>
    }
}

/// Pool list component
#[component]
fn PoolList() -> impl IntoView {
    view! {
        <div class="pool-list">
            <div class="pool-card">
                <h3>"Default Pool"</h3>
                <p>"Workers: 0"</p>
                <button>"Configure"</button>
            </div>
        </div>
    }
}

/// Providers page
#[component]
fn ProvidersPage() -> impl IntoView {
    view! {
        <div class="providers-page">
            <h1>"Providers"</h1>
            <p>"Manage worker providers"</p>
            <ProviderList />
        </div>
    }
}

/// Provider list component
#[component]
fn ProviderList() -> impl IntoView {
    view! {
        <div class="provider-list">
            <div class="provider-card">
                <h3>"Docker"</h3>
                <button>"Configure"</button>
            </div>
            <div class="provider-card">
                <h3>"Kubernetes"</h3>
                <button>"Configure"</button>
            </div>
        </div>
    }
}

/// Settings page
#[component]
fn SettingsPage() -> impl IntoView {
    view! {
        <div class="settings-page">
            <h1>"Settings"</h1>
            <SettingsForm />
        </div>
    }
}

/// Settings form component
#[component]
fn SettingsForm() -> impl IntoView {
    view! {
        <form class="settings-form">
            <div class="form-group">
                <label>"Default Namespace"</label>
                <input type="text" value="hodei-system" />
            </div>
            <div class="form-group">
                <label>"Job Timeout (seconds)"</label>
                <input type="number" value="3600" />
            </div>
            <button type="button">"Save Settings"</button>
        </form>
    }
}
