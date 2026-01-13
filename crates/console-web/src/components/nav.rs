//! Navigation bar component

use leptos::prelude::*;

/// Main navigation bar component
#[component]
pub fn NavBar() -> impl IntoView {
    view! {
        <nav class="navbar">
            <div class="navbar-brand">
                <a href="/" class="navbar-logo">
                    <span class="logo-icon material-symbols-outlined">"bolt"</span>
                    <span>"Hodei Console"</span>
                </a>
            </div>
            <div class="header-actions">
                <div class="header-search">
                    <span class="material-symbols-outlined header-search-icon">"search"</span>
                    <input type="text" placeholder="Search jobs, workers..." />
                </div>
                <button class="nav-btn" aria-label="Notifications">
                    <span class="material-symbols-outlined">"notifications"</span>
                </button>
                <button class="nav-btn" aria-label="User profile">
                    <span class="material-symbols-outlined">"account_circle"</span>
                </button>
            </div>
        </nav>
    }
}
