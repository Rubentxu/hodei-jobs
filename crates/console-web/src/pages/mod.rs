//! Routes module for Leptos Router
//!
//! Provides routing configuration for the console web application.

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::components::Layout;

// Page modules
mod dashboard;
mod jobs;
mod providers;
mod scheduler;
mod settings;
mod workers;

use dashboard::Dashboard;
use jobs::Jobs;
use providers::Providers;
use scheduler::Scheduler;
use settings::Settings;
use workers::Workers;

/// Main app layout with routing
#[component]
pub fn AppRouter() -> impl IntoView {
    view! {
        <Router>
            <Layout>
                <Routes fallback=|| view! { "404 - Page not found" }>
                    <Route path=path!("/") view=Dashboard />
                    <Route path=path!("/jobs") view=Jobs />
                    <Route path=path!("/jobs/:id") view=JobDetailPage />
                    <Route path=path!("/workers") view=Workers />
                    <Route path=path!("/scheduler") view=Scheduler />
                    <Route path=path!("/providers") view=Providers />
                    <Route path=path!("/settings") view=Settings />
                </Routes>
            </Layout>
        </Router>
    }
}

/// Job detail page placeholder
#[component]
fn JobDetailPage() -> impl IntoView {
    view! {
        <div class="page">
            <h1 class="page-title">"Job Details"</h1>
            <p class="page-subtitle">"Job details will be displayed here"</p>
        </div>
    }
}
