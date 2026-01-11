//! Hodei Jobs Administration Console
//!
//! A modern web administration console built with Leptos SSR that provides
//! a reactive UI for managing the Hodei Jobs Platform via gRPC.

use leptos::prelude::*;

pub mod components;
pub mod grpc;
pub mod pages;
pub mod types;

/// Application state for managing connection status
#[derive(Clone, Debug)]
pub struct AppState {
    pub grpc_connected: RwSignal<bool>,
    pub grpc_address: RwSignal<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            grpc_connected: RwSignal::new(false),
            grpc_address: RwSignal::new("http://localhost:50051".to_string()),
        }
    }
}

/// Provide global app state
#[component]
pub fn AppStateProvider(children: Children) -> impl IntoView {
    let state = AppState::new();
    provide_context(state);
    children()
}

/// The main application component
#[component]
pub fn App() -> impl IntoView {
    view! {
        <AppStateProvider>
            <pages::AppRouter />
        </AppStateProvider>
    }
}

/// Client-side entry point (WASM)
#[cfg(not(feature = "ssr"))]
#[wasm_bindgen(start)]
pub async fn main() {
    console_error_panic_hook::set_once();
    leptos::mount_to_body(|| view! { <App /> });
}
