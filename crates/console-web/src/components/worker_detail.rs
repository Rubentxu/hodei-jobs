//! Worker Detail Slide-over Panel

use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct WorkerDetail {
    pub id: String,
    pub name: String,
    pub status: WorkerDetailStatus,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkerDetailStatus {
    Online,
    Busy,
    Offline,
    Error,
}

#[component]
pub fn WorkerDetailPanel(worker: Option<WorkerDetail>, on_close: Action<(), ()>) -> impl IntoView {
    view! {
        <Show when=move || worker.is_some()>
            <div class="fixed inset-0 z-50 overflow-hidden">
                <div class="absolute inset-0 bg-gray-900/50" on:click=move |_| { on_close.dispatch(()); }></div>
                <div class="fixed inset-y-0 right-0 flex max-w-full pl-10">
                    <div class="w-screen max-w-md bg-white shadow-xl">
                        <div class="flex flex-col h-full">
                            <div class="px-4 py-6 border-b border-gray-200 bg-gray-50">
                                <div class="flex items-start justify-between">
                                    <h2 class="text-lg font-semibold text-gray-900">"Worker Details"</h2>
                                    <button class="p-1" on:click=move |_| { on_close.dispatch(()); }>
                                        <span class="material-symbols-outlined">"close"</span>
                                    </button>
                                </div>
                            </div>
                            <div class="flex-1 overflow-y-auto p-6">
                                <div class="flex items-center gap-3 mb-6">
                                    <div class="w-3 h-3 rounded-full bg-green-500"></div>
                                    <div>
                                        <h3 class="text-xl font-medium text-gray-900">"Worker Name"</h3>
                                        <p class="text-sm text-gray-500">"Online"</p>
                                    </div>
                                </div>
                                <div class="grid grid-cols-2 gap-4 mb-6">
                                    <div class="bg-gray-50 rounded-lg p-4">
                                        <p class="text-xs text-gray-500">"CPU"</p>
                                        <p class="text-2xl font-semibold">"45%"</p>
                                    </div>
                                    <div class="bg-gray-50 rounded-lg p-4">
                                        <p class="text-xs text-gray-500">"Memory"</p>
                                        <p class="text-2xl font-semibold">"62%"</p>
                                    </div>
                                </div>
                            </div>
                            <div class="px-4 py-4 border-t border-gray-200 bg-gray-50">
                                <div class="flex gap-3">
                                    <button class="flex-1 px-4 py-2 bg-primary text-white rounded-lg">"SSH"</button>
                                    <button class="flex-1 px-4 py-2 border rounded-lg">"Logs"</button>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}
