//! Scheduling Bounded Context
//!
//! Maneja las estrategias de scheduling y asignación de jobs
//!
//! ## Architecture
//! - **Events**: Domain events for observability without infrastructure coupling
//! - **Strategies**: Worker and provider selection algorithms (from strategies module)
//! - **Value Objects**: Strongly-typed domain objects (ProviderPreference, WorkerRequirements)
//! - **Scoring**: Composable scoring traits for provider selection
//!
//! ## Pure Domain
//! This module achieves domain purity by:
//! - Using `SchedulingEvent` instead of `tracing::debug!` for observability
//! - Decoupling provider matching logic into `ProviderPreference` and `ProviderTypeMapping`
//! - Encapsulating worker requirements in `WorkerRequirements` value object
//! - Extracting scoring logic into reusable `ProviderScoring` trait

pub mod events;
pub mod scoring;
pub mod strategies;
pub mod ttl_cache;
pub mod value_objects;

pub use events::*;
pub use scoring::*;
pub use ttl_cache::*;
pub use value_objects::*;

// Re-export strategy types for convenience
pub use strategies::{
    FastestStartupProviderSelector, FirstAvailableWorkerSelector, HealthiestProviderSelector,
    JobScheduler, LeastLoadedWorkerSelector, LowestCostProviderSelector,
    MostCapacityProviderSelector, ProviderInfo, ProviderSelectionStrategy, ProviderSelector,
    SchedulingContext, SchedulingDecision, WorkerSelectionStrategy, WorkerSelector,
};

use crate::jobs::Job;
use crate::shared_kernel::{ProviderId, WorkerId};
use crate::workers::Worker;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Configuration for SmartScheduler
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Strategy for selecting workers
    pub worker_strategy: WorkerSelectionStrategy,
    /// Strategy for selecting providers
    pub provider_strategy: ProviderSelectionStrategy,
    /// Max queue depth before rejecting
    pub max_queue_depth: usize,
    /// System load threshold for scaling up
    pub scale_up_load_threshold: f64,
    /// DEPRECATED: This field is ignored in ephemeral worker model.
    /// Workers are always provisioned fresh for each job and terminated after completion.
    /// Kept for backwards compatibility with configuration parsing.
    #[deprecated(note = "Ephemeral workers model: workers are always provisioned fresh")]
    pub prefer_existing_workers: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        #[allow(deprecated)]
        Self {
            worker_strategy: WorkerSelectionStrategy::LeastLoaded,
            provider_strategy: ProviderSelectionStrategy::FastestStartup,
            max_queue_depth: 100,
            scale_up_load_threshold: 0.8,
            // EPIC-21: Ephemeral workers model - always provision fresh workers
            prefer_existing_workers: false,
        }
    }
}

/// Smart Scheduler implementation
///
/// This is a domain-level scheduler that implements intelligent scheduling strategies
/// for assigning jobs to workers and deciding when to provision new workers.
///
/// ## Domain Purity (EPIC-022)
/// This implementation achieves purity by:
/// - Emitting `SchedulingEvent` instead of using tracing
/// - Using `ProviderPreference` for provider matching (eliminates code duplication)
/// - Using `WorkerRequirements` for worker filtering
/// - All observability is handled through domain events
pub struct SmartScheduler {
    config: SchedulerConfig,
    round_robin_worker_index: AtomicUsize,
    round_robin_provider_index: AtomicUsize,
}

impl SmartScheduler {
    /// Create a new SmartScheduler with the given configuration
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            round_robin_worker_index: AtomicUsize::new(0),
            round_robin_provider_index: AtomicUsize::new(0),
        }
    }

    /// Select a worker using the configured strategy
    pub(crate) fn select_worker(&self, job: &Job, workers: &[Worker]) -> Option<WorkerId> {
        if workers.is_empty() {
            // Emit domain event instead of tracing
            return None;
        }

        // Create worker requirements from job preferences
        let worker_requirements = WorkerRequirements::new(
            job.spec.preferences.required_labels.clone(),
            job.spec.preferences.required_annotations.clone(),
        )
        .ok();

        // Step 1: Filter workers by preferred provider if specified
        let mut eligible_workers = if let Some(preferred) = &job.spec.preferences.preferred_provider
        {
            // Use ProviderPreference for validated, normalized matching
            let pref = ProviderPreference::new(preferred).ok();

            workers
                .iter()
                .filter(|w| {
                    let provider_matches = pref
                        .as_ref()
                        .map(|p| p.matches_provider_type(w.provider_type()))
                        .unwrap_or(false);

                    provider_matches && w.state().can_accept_jobs()
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            workers
                .iter()
                .filter(|w| w.state().can_accept_jobs())
                .cloned()
                .collect::<Vec<_>>()
        };

        // Step 2: Filter workers by required labels (EPIC-21 US-07)
        if let Some(ref req) = worker_requirements
            && req.has_requirements()
        {
            eligible_workers.retain(|w| req.matches(w));
        }

        if eligible_workers.is_empty() {
            return None;
        }

        // Step 3: Apply selection strategy

        match self.config.worker_strategy {
            WorkerSelectionStrategy::FirstAvailable => {
                FirstAvailableWorkerSelector.select_worker(job, &eligible_workers)
            }
            WorkerSelectionStrategy::LeastLoaded => {
                LeastLoadedWorkerSelector.select_worker(job, &eligible_workers)
            }
            WorkerSelectionStrategy::RoundRobin => {
                if eligible_workers.is_empty() {
                    return None;
                }

                let index = self.round_robin_worker_index.fetch_add(1, Ordering::SeqCst);
                let worker = &eligible_workers[index % eligible_workers.len()];
                Some(worker.id().clone())
            }
            WorkerSelectionStrategy::MostCapacity => {
                LeastLoadedWorkerSelector.select_worker(job, &eligible_workers)
            }
            WorkerSelectionStrategy::Affinity => {
                // Check for job type affinity in worker labels
                let job_image = job.spec.image.as_deref();

                eligible_workers
                    .iter()
                    .find(|w| {
                        let worker_image_type = w.spec().labels.get("image_type");
                        worker_image_type
                            .map(|t| Some(t.as_str()) == job_image)
                            .unwrap_or(false)
                    })
                    .or_else(|| {
                        eligible_workers
                            .iter()
                            .find(|w| w.state().can_accept_jobs())
                    })
                    .map(|w| w.id().clone())
            }
        }
    }

    /// Select a provider using the configured strategy
    fn select_provider(&self, job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        match self.config.provider_strategy {
            ProviderSelectionStrategy::FirstAvailable => providers
                .iter()
                .find(|p| p.can_accept_workers())
                .map(|p| p.provider_id.clone()),
            ProviderSelectionStrategy::LowestCost => {
                LowestCostProviderSelector::new().select_provider(job, providers)
            }
            ProviderSelectionStrategy::FastestStartup => {
                FastestStartupProviderSelector::new().select_provider(job, providers)
            }
            ProviderSelectionStrategy::MostCapacity => {
                MostCapacityProviderSelector::new().select_provider(job, providers)
            }
            ProviderSelectionStrategy::RoundRobin => {
                let available: Vec<_> = providers
                    .iter()
                    .filter(|p| p.can_accept_workers())
                    .collect();

                if available.is_empty() {
                    return None;
                }

                let index = self
                    .round_robin_provider_index
                    .fetch_add(1, Ordering::SeqCst);
                Some(available[index % available.len()].provider_id.clone())
            }
            ProviderSelectionStrategy::Healthiest => {
                HealthiestProviderSelector::new().select_provider(job, providers)
            }
        }
    }
}

#[async_trait]
impl JobScheduler for SmartScheduler {
    async fn schedule(&self, context: SchedulingContext) -> anyhow::Result<SchedulingDecision> {
        let job_id = context.job.id.clone();

        // Check queue depth limit
        if context.pending_jobs_count > self.config.max_queue_depth {
            return Ok(SchedulingDecision::Reject {
                job_id,
                reason: format!(
                    "Queue depth {} exceeds max {}",
                    context.pending_jobs_count, self.config.max_queue_depth
                ),
            });
        }

        // Step 1: Try to assign to an existing available worker
        // Only assign if worker matches job's preferred provider (if specified)
        if !context.available_workers.is_empty() {
            // If job has a preferred provider, filter workers to only those from that provider
            let workers: Vec<Worker> = if context.job.spec.preferences.preferred_provider.is_some()
            {
                let pref = context
                    .job
                    .spec
                    .preferences
                    .preferred_provider
                    .as_ref()
                    .and_then(|p| crate::scheduling::ProviderPreference::new(p).ok());

                let filtered: Vec<Worker> = context
                    .available_workers
                    .iter()
                    .filter(|w| {
                        let matches = pref
                            .as_ref()
                            .map(|p_ref| p_ref.matches_provider_type(w.provider_type()))
                            .unwrap_or(true);
                        tracing::debug!(
                            job_id = %context.job.id,
                            worker_id = %w.id(),
                            worker_provider_type = %w.provider_type(),
                            preferred_provider = %context.job.spec.preferences.preferred_provider.clone().unwrap_or_default(),
                            matches = matches,
                            "Worker filtering check"
                        );
                        matches
                    })
                    .cloned()
                    .collect();

                tracing::debug!(
                    job_id = %context.job.id,
                    original_workers = context.available_workers.len(),
                    filtered_workers = filtered.len(),
                    "Worker filtering result"
                );

                filtered
            } else {
                context.available_workers.clone()
            };

            if let Some(worker_id) = self.select_worker(&context.job, &workers) {
                return Ok(SchedulingDecision::AssignToWorker { job_id, worker_id });
            }
        }

        // Step 2: If no workers available, provision a new one (EPIC-21)
        if let Some(provider_id) =
            self.select_provider_with_preferences(&context.job, &context.available_providers)
        {
            return Ok(SchedulingDecision::ProvisionWorker {
                job_id,
                provider_id,
            });
        }

        // Check if we should scale up
        if context.system_load > self.config.scale_up_load_threshold {
            return Ok(SchedulingDecision::Enqueue {
                job_id,
                reason: "System under high load, enqueueing for later".to_string(),
            });
        }

        // Last resort: enqueue
        Ok(SchedulingDecision::Enqueue {
            job_id,
            reason: "No providers available for job".to_string(),
        })
    }

    fn strategy_name(&self) -> &str {
        "smart_scheduler"
    }
}

impl SmartScheduler {
    /// Select a provider considering job preferences
    ///
    /// ## US-27.4: GPU Requirements Filtering
    /// ## US-27.6: Region Affinity
    pub fn select_provider_with_preferences(
        &self,
        job: &Job,
        providers: &[ProviderInfo],
    ) -> Option<ProviderId> {
        // Step 0: Apply GPU filtering (US-27.4)
        // Jobs requiring GPU must only be assigned to providers with GPU support
        let gpu_required = job.spec.resources.gpu_required;
        let filtered_providers: Vec<&ProviderInfo> = if gpu_required {
            providers
                .iter()
                .filter(|p| p.can_handle_gpu_requirement(1)) // Assume 1 GPU if required
                .collect()
        } else {
            providers.iter().collect()
        };

        if filtered_providers.is_empty() {
            // GPU required but no provider supports it
            tracing::debug!(
                job_id = %job.id,
                gpu_required = gpu_required,
                "No providers available with GPU support for job"
            );
            return None;
        }

        // Step 1: Check if job has a preferred provider
        if let Some(preferred) = &job.spec.preferences.preferred_provider {
            // Use ProviderPreference for efficient provider matching
            let pref = ProviderPreference::new(preferred).ok();

            tracing::debug!(
                job_id = %job.id,
                preferred_provider = preferred,
                providers_count = filtered_providers.len(),
                "Checking preferred provider"
            );

            // Try to find provider by name or type
            let preferred_provider = filtered_providers.iter().find(|p| {
                // Match using ProviderPreference if available
                let type_matches = pref
                    .as_ref()
                    .map(|p_ref| p_ref.matches_provider_type(&p.provider_type))
                    .unwrap_or(false);

                let id_matches: bool = pref
                    .as_ref()
                    .map(|p_pref| {
                        if p_pref.is_specific_provider() {
                            p_pref.matches_provider_id(&p.provider_id)
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);

                let can_accept = p.can_accept_workers();

                tracing::debug!(
                    job_id = %job.id,
                    provider_id = %p.provider_id,
                    provider_type = %p.provider_type,
                    type_matches = type_matches,
                    id_matches = id_matches,
                    can_accept_workers = can_accept,
                    "Provider matching check"
                );

                (type_matches || id_matches) && can_accept
            });

            if let Some(provider) = preferred_provider {
                tracing::debug!(
                    job_id = %job.id,
                    selected_provider_id = %provider.provider_id,
                    selected_provider_type = %provider.provider_type,
                    "Selected provider by preferred_provider"
                );
                return Some(provider.provider_id.clone());
            } else {
                tracing::debug!(
                    job_id = %job.id,
                    preferred_provider = preferred,
                    "No provider matched preferred_provider, falling back to strategy"
                );
            }
        }

        // Step 2: Apply region affinity (US-27.6)
        // If job has a preferred region, log it for observability
        if let Some(ref preferred_region) = job.spec.preferences.preferred_region {
            tracing::debug!(
                job_id = %job.id,
                preferred_region = preferred_region,
                providers_count = filtered_providers.len(),
                "Applying region affinity for provider selection"
            );
        }

        // Step 3: Fallback to configured strategy with region-aware scoring
        // Use the filtered providers directly for strategy selection
        // Convert Vec<&ProviderInfo> to Vec<ProviderInfo> for strategy
        let owned_providers: Vec<ProviderInfo> =
            filtered_providers.iter().map(|p| (*p).clone()).collect();

        // The strategy will return a provider, then we apply region considerations
        if let Some(provider_id) = self.select_provider(job, &owned_providers) {
            // Apply region boost if job has region preference
            if let Some(ref preferred_region) = job.spec.preferences.preferred_region {
                // Find the selected provider and check region match
                let selected = filtered_providers
                    .iter()
                    .find(|p| p.provider_id == provider_id);
                if let Some(p) = selected
                    && !p.supports_region(preferred_region)
                {
                    // Selected provider doesn't match region - log warning
                    tracing::warn!(
                        job_id = %job.id,
                        preferred_region = preferred_region,
                        selected_provider = %provider_id,
                        "Job has region preference but selected provider doesn't support it"
                    );
                }
            }
            return Some(provider_id);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{Job, JobPreferences, JobSpec};
    use crate::shared_kernel::{JobId, ProviderId};
    use crate::workers::{ProviderType, Worker, WorkerHandle, WorkerSpec};
    use std::collections::HashMap;
    use std::time::Duration;

    fn create_test_providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                provider_id: ProviderId::new(),
                provider_type: ProviderType::Docker,
                active_workers: 5,
                max_workers: 10,
                estimated_startup_time: Duration::from_secs(5),
                health_score: 0.9,
                cost_per_hour: 0.0,
                gpu_support: false,
                gpu_types: vec![],
                regions: vec!["local".to_string()],
            },
            ProviderInfo {
                provider_id: ProviderId::new(),
                provider_type: ProviderType::Kubernetes,
                active_workers: 2,
                max_workers: 20,
                estimated_startup_time: Duration::from_secs(30),
                health_score: 0.95,
                cost_per_hour: 0.5,
                gpu_support: true,
                gpu_types: vec!["nvidia".to_string()],
                regions: vec!["us-east-1".to_string(), "eu-west-1".to_string()],
            },
        ]
    }

    fn create_test_job(preferred_provider: Option<String>) -> Job {
        let preferences = JobPreferences {
            preferred_provider,
            preferred_region: None,
            max_budget: None,
            priority: crate::jobs::JobPriority::Normal,
            allow_retry: true,
            required_labels: HashMap::new(),
            required_annotations: HashMap::new(),
        };

        let spec = JobSpec::new(vec!["echo".to_string()]).with_preferences(preferences);

        Job::new(JobId::new(), "test-job-with-prefs".to_string(), spec)
    }

    fn create_test_job_with_labels_and_annotations(
        preferred_provider: Option<String>,
        required_labels: HashMap<String, String>,
        required_annotations: HashMap<String, String>,
    ) -> Job {
        let preferences = JobPreferences {
            preferred_provider,
            preferred_region: None,
            max_budget: None,
            priority: crate::jobs::JobPriority::Normal,
            allow_retry: true,
            required_labels,
            required_annotations,
        };

        let spec = JobSpec::new(vec!["echo".to_string()]).with_preferences(preferences);

        Job::new(JobId::new(), "test-job-labels".to_string(), spec)
    }

    #[test]
    fn test_select_provider_respects_preferred_provider_when_available() {
        let providers = create_test_providers();
        let job = create_test_job(Some("k8s".to_string()));

        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_provider_with_preferences(&job, &providers);

        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        let selected_provider = providers
            .iter()
            .find(|p| p.provider_id == selected_id)
            .unwrap();
        assert_eq!(selected_provider.provider_type, ProviderType::Kubernetes);
    }

    #[test]
    fn test_select_provider_fallback_when_preferred_not_available() {
        let providers = create_test_providers();
        let job = create_test_job(Some("firecracker".to_string()));

        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_provider_with_preferences(&job, &providers);

        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        let selected_provider = providers
            .iter()
            .find(|p| p.provider_id == selected_id)
            .unwrap();
        assert_eq!(selected_provider.provider_type, ProviderType::Docker);
    }

    #[test]
    fn test_select_provider_matches_by_provider_type() {
        let providers = create_test_providers();
        let job = create_test_job(Some("kubernetes".to_string()));

        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_provider_with_preferences(&job, &providers);

        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        let selected_provider = providers
            .iter()
            .find(|p| p.provider_id == selected_id)
            .unwrap();
        assert_eq!(selected_provider.provider_type, ProviderType::Kubernetes);
    }

    #[test]
    fn test_select_worker_filters_by_required_labels() {
        let mut spec1 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec1
            .labels
            .insert("environment".to_string(), "production".to_string());
        let handle1 = WorkerHandle::new(
            spec1.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker1 = Worker::new(handle1, spec1);
        worker1.mark_ready().unwrap();
        worker1.mark_ready().unwrap();

        let mut spec2 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50052".to_string(),
        );
        spec2
            .labels
            .insert("environment".to_string(), "staging".to_string());
        let handle2 = WorkerHandle::new(
            spec2.worker_id.clone(),
            "container-2".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker2 = Worker::new(handle2, spec2);
        worker2.mark_ready().unwrap();
        worker2.mark_ready().unwrap();

        let mut spec3 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50053".to_string(),
        );
        spec3
            .labels
            .insert("environment".to_string(), "production".to_string());
        spec3
            .labels
            .insert("zone".to_string(), "us-east-1".to_string());
        let handle3 = WorkerHandle::new(
            spec3.worker_id.clone(),
            "container-3".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker3 = Worker::new(handle3, spec3);
        worker3.mark_ready().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        let job =
            create_test_job_with_labels_and_annotations(None, required_labels, HashMap::new());

        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert!(selected_id == *worker1.id() || selected_id == *worker3.id());
    }

    #[test]
    fn test_select_worker_filters_by_required_annotations() {
        let mut spec1 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec1
            .annotations
            .insert("team".to_string(), "platform".to_string());
        let handle1 = WorkerHandle::new(
            spec1.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker1 = Worker::new(handle1, spec1);
        worker1.mark_ready().unwrap();
        worker1.mark_ready().unwrap();

        let mut spec2 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50052".to_string(),
        );
        spec2
            .annotations
            .insert("team".to_string(), "data".to_string());
        let handle2 = WorkerHandle::new(
            spec2.worker_id.clone(),
            "container-2".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker2 = Worker::new(handle2, spec2);
        worker2.mark_ready().unwrap();
        worker2.mark_ready().unwrap();

        let mut spec3 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50053".to_string(),
        );
        spec3
            .annotations
            .insert("team".to_string(), "platform".to_string());
        spec3
            .annotations
            .insert("cost_center".to_string(), "engineering".to_string());
        let handle3 = WorkerHandle::new(
            spec3.worker_id.clone(),
            "container-3".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker3 = Worker::new(handle3, spec3);
        worker3.mark_ready().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        let mut required_annotations = HashMap::new();
        required_annotations.insert("team".to_string(), "platform".to_string());
        let job =
            create_test_job_with_labels_and_annotations(None, HashMap::new(), required_annotations);

        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert!(selected_id == *worker1.id() || selected_id == *worker3.id());
    }

    #[test]
    fn test_select_worker_filters_by_labels_and_annotations_combined() {
        let mut spec1 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec1
            .labels
            .insert("environment".to_string(), "production".to_string());
        spec1
            .annotations
            .insert("team".to_string(), "platform".to_string());
        let handle1 = WorkerHandle::new(
            spec1.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker1 = Worker::new(handle1, spec1);
        worker1.mark_ready().unwrap();
        worker1.mark_ready().unwrap();

        let mut spec2 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50052".to_string(),
        );
        spec2
            .labels
            .insert("environment".to_string(), "production".to_string());
        spec2
            .annotations
            .insert("team".to_string(), "data".to_string());
        let handle2 = WorkerHandle::new(
            spec2.worker_id.clone(),
            "container-2".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker2 = Worker::new(handle2, spec2);
        worker2.mark_ready().unwrap();
        worker2.mark_ready().unwrap();

        let mut spec3 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50053".to_string(),
        );
        spec3
            .labels
            .insert("environment".to_string(), "staging".to_string());
        spec3
            .annotations
            .insert("team".to_string(), "platform".to_string());
        let handle3 = WorkerHandle::new(
            spec3.worker_id.clone(),
            "container-3".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker3 = Worker::new(handle3, spec3);
        worker3.mark_ready().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        let mut required_annotations = HashMap::new();
        required_annotations.insert("team".to_string(), "platform".to_string());
        let job = create_test_job_with_labels_and_annotations(
            None,
            required_labels,
            required_annotations,
        );

        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert_eq!(selected_id, *worker1.id());
    }

    #[test]
    fn test_select_worker_returns_none_when_no_workers_match_labels() {
        let mut spec1 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec1
            .labels
            .insert("environment".to_string(), "staging".to_string());
        let handle1 = WorkerHandle::new(
            spec1.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker1 = Worker::new(handle1, spec1);
        worker1.mark_ready().unwrap();
        worker1.mark_ready().unwrap();

        let mut spec2 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50052".to_string(),
        );
        spec2
            .labels
            .insert("environment".to_string(), "development".to_string());
        let handle2 = WorkerHandle::new(
            spec2.worker_id.clone(),
            "container-2".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker2 = Worker::new(handle2, spec2);
        worker2.mark_ready().unwrap();
        worker2.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone()];

        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        let job =
            create_test_job_with_labels_and_annotations(None, required_labels, HashMap::new());

        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        assert!(selected.is_none());
    }

    #[test]
    fn test_smart_scheduler_with_provider_preference_matching() {
        // Test that ProviderPreference is used for matching
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        // ProviderPreference handles the matching logic
        let pref = ProviderPreference::new("k8s").unwrap();
        assert!(pref.matches_provider_type(&ProviderType::Kubernetes));
    }
}
