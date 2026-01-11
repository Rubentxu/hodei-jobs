//! Hodei Jobs Administration Console
//!
//! A modern web administration console built with Leptos SSR that provides
//! a reactive UI for managing the Hodei Jobs Platform via gRPC.

#![recursion_limit = "256"]

use leptos::prelude::*;

#[cfg(feature = "ssr")]
pub mod server;

pub mod components;
pub mod grpc;
pub mod pages;
pub mod server_functions;
pub mod types;

/// Application state for managing connection status and theme
#[derive(Clone, Debug)]
pub struct AppState {
    pub grpc_connected: RwSignal<bool>,
    pub grpc_address: RwSignal<String>,
    pub current_theme: RwSignal<Theme>,
    pub current_user: RwSignal<Option<User>>,
}

/// Theme options
#[derive(Clone, Debug, PartialEq)]
pub enum Theme {
    Light,
    Dark,
}

/// Basic user info
#[derive(Clone, Debug)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            grpc_connected: RwSignal::new(false),
            grpc_address: RwSignal::new("http://localhost:50051".to_string()),
            current_theme: RwSignal::new(Theme::Light),
            current_user: RwSignal::new(None),
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
    leptos_meta::provide_meta_context();
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
