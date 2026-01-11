//! Worker Pools page - Configure worker pools

use leptos::prelude::*;

#[component]
pub fn WorkerPools() -> impl IntoView {
    view! {
        <div class="worker-pools-page">
            <h1>"Worker Pools"</h1>
            <p>"Configure worker pool scaling and resources"</p>
            <div class="pools-list">
                <div class="pool-card">
                    <h3>"Default Pool"</h3>
                    <button>"Configure"</button>
                </div>
            </div>
        </div>
    }
}
