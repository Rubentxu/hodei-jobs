//! Advanced State Management for Console Web
//!
//! Provides page-specific state management with:
//! - Selection support
//! - Sorting state

use crate::server_functions::{
    JobSummaryStatus, ProviderInfo, QueueItemInfo, RecentJob, WorkerInfo,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc = 0,
    Desc = 1,
}

impl Default for SortDirection {
    fn default() -> Self {
        Self::Asc
    }
}

/// Job filter criteria
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JobFilters {
    pub search_term: String,
    pub status_filter: String,
    pub provider_filter: String,
}

/// Jobs page state with selection and sorting
#[derive(Clone)]
pub struct JobsPageState {
    /// All loaded jobs
    pub jobs: RwSignal<Vec<RecentJob>>,
    /// Selected job IDs
    pub selected_ids: RwSignal<HashSet<String>>,
    /// Current filters
    pub filters: RwSignal<JobFilters>,
    /// Sort column name
    pub sort_column: RwSignal<Option<String>>,
    /// Sort direction
    pub sort_direction: RwSignal<SortDirection>,
    /// Total count from server
    pub total_count: RwSignal<i32>,
    /// Loading state
    pub is_loading: RwSignal<bool>,
}

impl JobsPageState {
    /// Create new jobs page state
    pub fn new() -> Self {
        Self {
            jobs: RwSignal::new(Vec::new()),
            selected_ids: RwSignal::new(HashSet::new()),
            filters: RwSignal::new(JobFilters::default()),
            sort_column: RwSignal::new(None),
            sort_direction: RwSignal::new(SortDirection::default()),
            total_count: RwSignal::new(0),
            is_loading: RwSignal::new(false),
        }
    }

    /// Toggle selection of a job
    pub fn toggle_selection(&self, job_id: &str) {
        let mut selected = self.selected_ids.get();
        if selected.contains(job_id) {
            selected.remove(job_id);
        } else {
            selected.insert(job_id.to_string());
        }
        self.selected_ids.set(selected);
    }

    /// Select all visible jobs
    pub fn select_all(&self) {
        let jobs = self.jobs.get();
        let mut selected = self.selected_ids.get();
        for job in &jobs {
            selected.insert(job.id.clone());
        }
        self.selected_ids.set(selected);
    }

    /// Clear all selections
    pub fn clear_selection(&self) {
        self.selected_ids.set(HashSet::new());
    }
}

impl Default for JobsPageState {
    fn default() -> Self {
        Self::new()
    }
}

/// Worker filter criteria
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkerFilters {
    pub search_term: String,
    pub status_filter: String,
    pub provider_filter: String,
}

/// Workers page state
#[derive(Clone)]
pub struct WorkersPageState {
    /// All loaded workers
    pub workers: RwSignal<Vec<WorkerInfo>>,
    /// Selected worker IDs
    pub selected_ids: RwSignal<HashSet<String>>,
    /// Current filters
    pub filters: RwSignal<WorkerFilters>,
    /// Loading state
    pub is_loading: RwSignal<bool>,
}

impl WorkersPageState {
    pub fn new() -> Self {
        Self {
            workers: RwSignal::new(Vec::new()),
            selected_ids: RwSignal::new(HashSet::new()),
            filters: RwSignal::new(WorkerFilters::default()),
            is_loading: RwSignal::new(false),
        }
    }

    pub fn toggle_selection(&self, worker_id: &str) {
        let mut selected = self.selected_ids.get();
        if selected.contains(worker_id) {
            selected.remove(worker_id);
        } else {
            selected.insert(worker_id.to_string());
        }
        self.selected_ids.set(selected);
    }

    pub fn clear_selection(&self) {
        self.selected_ids.set(HashSet::new());
    }
}

impl Default for WorkersPageState {
    fn default() -> Self {
        Self::new()
    }
}

/// Scheduler/Queue page state
#[derive(Clone)]
pub struct SchedulerPageState {
    /// Queue items
    pub queue_items: RwSignal<Vec<QueueItemInfo>>,
    /// Selected items
    pub selected_ids: RwSignal<HashSet<String>>,
    /// Loading state
    pub is_loading: RwSignal<bool>,
    /// Auto-refresh enabled
    pub auto_refresh: RwSignal<bool>,
    /// Refresh interval in seconds
    pub refresh_interval: RwSignal<u64>,
}

impl SchedulerPageState {
    pub fn new() -> Self {
        Self {
            queue_items: RwSignal::new(Vec::new()),
            selected_ids: RwSignal::new(HashSet::new()),
            is_loading: RwSignal::new(false),
            auto_refresh: RwSignal::new(true),
            refresh_interval: RwSignal::new(30),
        }
    }

    pub fn toggle_selection(&self, item_id: &str) {
        let mut selected = self.selected_ids.get();
        if selected.contains(item_id) {
            selected.remove(item_id);
        } else {
            selected.insert(item_id.to_string());
        }
        self.selected_ids.set(selected);
    }

    pub fn clear_selection(&self) {
        self.selected_ids.set(HashSet::new());
    }
}

impl Default for SchedulerPageState {
    fn default() -> Self {
        Self::new()
    }
}

/// Providers page state
#[derive(Clone)]
pub struct ProvidersPageState {
    /// All providers
    pub providers: RwSignal<Vec<ProviderInfo>>,
    /// Selected provider IDs
    pub selected_ids: RwSignal<HashSet<String>>,
    /// Loading state
    pub is_loading: RwSignal<bool>,
    /// Filter term
    pub search_term: RwSignal<String>,
}

impl ProvidersPageState {
    pub fn new() -> Self {
        Self {
            providers: RwSignal::new(Vec::new()),
            selected_ids: RwSignal::new(HashSet::new()),
            is_loading: RwSignal::new(false),
            search_term: RwSignal::new(String::new()),
        }
    }

    pub fn toggle_selection(&self, provider_id: &str) {
        let mut selected = self.selected_ids.get();
        if selected.contains(provider_id) {
            selected.remove(provider_id);
        } else {
            selected.insert(provider_id.to_string());
        }
        self.selected_ids.set(selected);
    }

    pub fn clear_selection(&self) {
        self.selected_ids.set(HashSet::new());
    }
}

impl Default for ProvidersPageState {
    fn default() -> Self {
        Self::new()
    }
}

/// Global state provider component
#[component]
pub fn StateProvider(children: Children) -> impl IntoView {
    // Create page states
    let jobs_state = JobsPageState::new();
    let workers_state = WorkersPageState::new();
    let scheduler_state = SchedulerPageState::new();
    let providers_state = ProvidersPageState::new();

    // Provide to context
    provide_context(jobs_state);
    provide_context(workers_state);
    provide_context(scheduler_state);
    provide_context(providers_state);

    children()
}

/// Hook to get jobs page state
pub fn use_jobs_state() -> JobsPageState {
    let ctx = use_context::<JobsPageState>();
    ctx.unwrap_or_default()
}

/// Hook to get workers page state
pub fn use_workers_state() -> WorkersPageState {
    let ctx = use_context::<WorkersPageState>();
    ctx.unwrap_or_default()
}

/// Hook to get scheduler page state
pub fn use_scheduler_state() -> SchedulerPageState {
    let ctx = use_context::<SchedulerPageState>();
    ctx.unwrap_or_default()
}

/// Hook to get providers page state
pub fn use_providers_state() -> ProvidersPageState {
    let ctx = use_context::<ProvidersPageState>();
    ctx.unwrap_or_default()
}
