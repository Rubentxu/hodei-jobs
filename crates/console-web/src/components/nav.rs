//! Navigation bar component

use leptos::prelude::*;

#[component]
pub fn NavBar() -> impl IntoView {
    view! {
        <nav class="navbar">
            <div class="nav-brand">
                <a href="/">"Hodei Console"</a>
            </div>
            <ul class="nav-links">
                <li><a href="/">"Dashboard"</a></li>
                <li><a href="/jobs">"Jobs"</a></li>
                <li><a href="/pools">"Worker Pools"</a></li>
                <li><a href="/providers">"Providers"</a></li>
                <li><a href="/settings">"Settings"</a></li>
            </ul>
        </nav>
    }
}
