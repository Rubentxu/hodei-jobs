//! Global State Store for the Console Web

use crate::types::*;
use leptos::prelude::*;

/// App state using signals
#[derive(Clone)]
pub struct AppState {
    pub providers: ReadSignal<Vec<Provider>>,
    pub worker_pools: ReadSignal<Vec<WorkerPool>>,
    pub jobs: ReadSignal<Vec<Job>>,
    pub settings: ReadSignal<OperatorSettings>,
}

/// Get the app state from context
pub fn use_app_state() -> AppState {
    let ctx = use_context::<AppState>();
    if let Some(state) = ctx {
        state
    } else {
        panic!("App state not found. Wrap your app in AppStateProvider.");
    }
}

#[component]
pub fn AppStateProvider(children: Children) -> impl IntoView {
    let (providers, _) = create_signal(vec![]);
    let (worker_pools, _) = create_signal(vec![]);
    let (jobs, _) = create_signal(vec![]);

    let default_settings = OperatorSettings {
        namespace: "hodei-system".to_string(),
        watch_namespace: "".to_string(),
        server_address: "hodei-server:50051".to_string(),
        log_level: LogLevel::Info,
        health_check_interval: 30,
    };
    let (settings, _) = create_signal(default_settings);

    provide_context(AppState {
        providers,
        worker_pools,
        jobs,
        settings,
    });

    children()
}
