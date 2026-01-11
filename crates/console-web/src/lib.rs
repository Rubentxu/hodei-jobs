//! Hodei Jobs Administration Console
//!
//! Simplified version for Leptos 0.8.x SSR

use leptos::prelude::*;

pub mod components;
pub mod pages;
pub mod types;

pub use types::*;

/// The main application component
#[component]
pub fn App() -> impl IntoView {
    view! {
        <div class="app">
            <components::NavBar />
            <main>
                <pages::Dashboard />
            </main>
        </div>
    }
}

/// Client-side entry point (WASM)
#[cfg(not(feature = "ssr"))]
#[wasm_bindgen(start)]
pub async fn main() {
    console_error_panic_hook::set_once();
    leptos::mount_to_body(|| view! { <App /> });
}
