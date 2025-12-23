//! Scheduling Bounded Context
//!
//! Maneja las estrategias de scheduling y asignación de jobs

pub mod strategies;

pub use strategies::*;

use crate::jobs::Job;
use crate::shared_kernel::{ProviderId, Result, WorkerId};
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

/// Smart Scheduler implementation
///
/// This is a domain-level scheduler that implements intelligent scheduling strategies
/// for assigning jobs to workers and deciding when to provision new workers.
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
            tracing::debug!(
                job_id = %job.id,
                "No workers available for job scheduling"
            );
            return None;
        }

        // Step 1: Filter workers by preferred provider if specified
        let mut eligible_workers = if let Some(preferred) = &job.spec.preferences.preferred_provider
        {
            tracing::debug!(
                job_id = %job.id,
                preferred_provider = %preferred,
                "Filtering workers by preferred provider"
            );

            let preferred_lower = preferred.to_lowercase();
            workers
                .iter()
                .filter(|w| {
                    let provider_type_matches = w.provider_type().to_string().to_lowercase()
                        == preferred_lower
                        || w.provider_type()
                            .to_string()
                            .to_lowercase()
                            .contains(&preferred_lower)
                        || preferred_lower.contains(&w.provider_type().to_string().to_lowercase())
                        || (preferred_lower == "k8s"
                            && w.provider_type().to_string().to_lowercase() == "kubernetes")
                        || (preferred_lower == "kube"
                            && w.provider_type().to_string().to_lowercase() == "kubernetes");

                    provider_type_matches && w.state().can_accept_jobs()
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            // No preference, consider all available workers
            workers
                .iter()
                .filter(|w| w.state().can_accept_jobs())
                .cloned()
                .collect::<Vec<_>>()
        };

        // Step 2: Filter workers by required labels (EPIC-21 US-07)
        if !job.spec.preferences.required_labels.is_empty() {
            tracing::debug!(
                job_id = %job.id,
                required_labels = ?job.spec.preferences.required_labels,
                "Filtering workers by required labels"
            );

            eligible_workers.retain(|w| {
                job.spec
                    .preferences
                    .required_labels
                    .iter()
                    .all(|(key, value)| w.spec().labels.get(key) == Some(value))
            });
        }

        // Step 3: Filter workers by required annotations (EPIC-21 US-07)
        if !job.spec.preferences.required_annotations.is_empty() {
            tracing::debug!(
                job_id = %job.id,
                required_annotations = ?job.spec.preferences.required_annotations,
                "Filtering workers by required annotations"
            );

            eligible_workers.retain(|w| {
                job.spec
                    .preferences
                    .required_annotations
                    .iter()
                    .all(|(key, value)| w.spec().annotations.get(key) == Some(value))
            });
        }

        if eligible_workers.is_empty() {
            tracing::debug!(
                job_id = %job.id,
                "No eligible workers after filtering"
            );
            return None;
        }

        let available_count = eligible_workers.len();
        tracing::debug!(
            job_id = %job.id,
            total_workers = workers.len(),
            eligible_workers = available_count,
            strategy = ?self.config.worker_strategy,
            "Evaluating workers for job scheduling"
        );

        // Log worker details
        for (idx, worker) in eligible_workers.iter().enumerate() {
            tracing::debug!(
                job_id = %job.id,
                worker_id = %worker.id(),
                worker_state = ?worker.state(),
                worker_provider = ?worker.provider_type(),
                "Checking worker {} for job",
                idx + 1
            );
        }

        let result = match self.config.worker_strategy {
            WorkerSelectionStrategy::FirstAvailable => {
                FirstAvailableWorkerSelector.select_worker(job, &eligible_workers)
            }
            WorkerSelectionStrategy::LeastLoaded => {
                LeastLoadedWorkerSelector.select_worker(job, &eligible_workers)
            }
            WorkerSelectionStrategy::RoundRobin => {
                if eligible_workers.is_empty() {
                    tracing::debug!(job_id = %job.id, "No available workers for RoundRobin strategy");
                    return None;
                }

                let index = self.round_robin_worker_index.fetch_add(1, Ordering::SeqCst);
                let worker = &eligible_workers[index % eligible_workers.len()];
                Some(worker.id().clone())
            }
            WorkerSelectionStrategy::MostCapacity => {
                // For now, same as LeastLoaded (could be enhanced with resource metrics)
                LeastLoadedWorkerSelector.select_worker(job, &eligible_workers)
            }
            WorkerSelectionStrategy::Affinity => {
                // Check for job type affinity in worker labels
                // Falls back to first available if no affinity match
                let job_image = job.spec.image.as_deref();
                tracing::debug!(
                    job_id = %job.id,
                    job_image = ?job_image,
                    "Using Affinity strategy - checking for image affinity"
                );

                let result = eligible_workers
                    .iter()
                    .find(|w| {
                        // Check if worker has matching capabilities for job image
                        let worker_image_type = w.spec().labels.get("image_type");
                        let matches = worker_image_type
                            .map(|t| Some(t.as_str()) == job_image)
                            .unwrap_or(false);

                        tracing::debug!(
                            job_id = %job.id,
                            worker_id = %w.id(),
                            worker_image_type = ?worker_image_type,
                            job_image = ?job_image,
                            affinity_match = matches,
                            "Checking affinity match"
                        );

                        matches
                    })
                    .or_else(|| {
                        tracing::debug!(
                            job_id = %job.id,
                            "No affinity match found, falling back to first available worker"
                        );
                        eligible_workers
                            .iter()
                            .find(|w| w.state().can_accept_jobs())
                    })
                    .map(|w| w.id().clone());

                result
            }
        };

        if let Some(ref worker_id) = result {
            tracing::debug!(
                job_id = %job.id,
                selected_worker_id = %worker_id,
                "Worker selected successfully"
            );
        } else {
            tracing::debug!(
                job_id = %job.id,
                "No worker could be selected"
            );
        }

        result
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
                LowestCostProviderSelector.select_provider(job, providers)
            }
            ProviderSelectionStrategy::FastestStartup => {
                FastestStartupProviderSelector.select_provider(job, providers)
            }
            ProviderSelectionStrategy::MostCapacity => {
                MostCapacityProviderSelector.select_provider(job, providers)
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
                HealthiestProviderSelector.select_provider(job, providers)
            }
        }
    }
}

#[async_trait]
impl JobScheduler for SmartScheduler {
    async fn schedule(&self, context: SchedulingContext) -> Result<SchedulingDecision> {
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
        if !context.available_workers.is_empty() {
            if let Some(worker_id) = self.select_worker(&context.job, &context.available_workers) {
                tracing::info!(
                    job_id = %job_id,
                    worker_id = %worker_id,
                    available_workers = context.available_workers.len(),
                    "Assigning job to existing worker"
                );
                return Ok(SchedulingDecision::AssignToWorker { job_id, worker_id });
            }
        }

        // Step 2: If no workers available, provision a new one (EPIC-21)
        if let Some(provider_id) =
            self.select_provider_with_preferences(&context.job, &context.available_providers)
        {
            tracing::info!(
                job_id = %job_id,
                provider_id = %provider_id,
                preferred_provider = ?context.job.spec.preferences.preferred_provider,
                available_workers = context.available_workers.len(),
                "No workers available, provisioning ephemeral worker"
            );
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
    pub fn select_provider_with_preferences(
        &self,
        job: &Job,
        providers: &[ProviderInfo],
    ) -> Option<ProviderId> {
        // Step 1: Check if job has a preferred provider
        if let Some(preferred) = &job.spec.preferences.preferred_provider {
            tracing::debug!(
                job_id = %job.id,
                preferred_provider = %preferred,
                "Job has preferred provider, checking availability"
            );

            // Try to find provider by name or type
            let preferred_lower = preferred.to_lowercase();
            let preferred_provider = providers.iter().find(|p| {
                // Match by provider type name (e.g., "k8s" -> "kubernetes", "kube" -> "kubernetes")
                let provider_type_str = p.provider_type.to_string().to_lowercase();
                let provider_type_matches = provider_type_str == preferred_lower
                    || provider_type_str.contains(&preferred_lower)
                    || preferred_lower.contains(&provider_type_str)
                    // Common aliases
                    || (preferred_lower == "k8s" && provider_type_str == "kubernetes")
                    || (preferred_lower == "kube" && provider_type_str == "kubernetes")
                    // Match by exact provider_id string
                    || format!("{}", p.provider_id).to_lowercase() == preferred_lower;

                provider_type_matches && p.can_accept_workers()
            });

            if let Some(provider) = preferred_provider {
                tracing::debug!(
                    job_id = %job.id,
                    selected_provider_id = %provider.provider_id,
                    selected_provider_type = ?provider.provider_type,
                    "Selected preferred provider"
                );
                return Some(provider.provider_id.clone());
            }

            tracing::warn!(
                job_id = %job.id,
                preferred_provider = %preferred,
                "Preferred provider not available, falling back to strategy"
            );
        }

        // Step 2: Fallback to configured strategy
        self.select_provider(job, providers)
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
            },
            ProviderInfo {
                provider_id: ProviderId::new(),
                provider_type: ProviderType::Kubernetes,
                active_workers: 2,
                max_workers: 20,
                estimated_startup_time: Duration::from_secs(30),
                health_score: 0.95,
                cost_per_hour: 0.5,
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

        Job::new(JobId::new(), spec)
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

        Job::new(JobId::new(), spec)
    }

    #[test]
    fn test_select_provider_respects_preferred_provider_when_available() {
        // GIVEN: Job con preferred_provider="k8s"
        let providers = create_test_providers();
        let job = create_test_job(Some("k8s".to_string()));

        // WHEN: select_provider_with_preferences es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_provider_with_preferences(&job, &providers);

        // THEN: Debe retornar Kubernetes provider
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
        // GIVEN: Job con preferred_provider="firecracker"
        let providers = create_test_providers();
        let job = create_test_job(Some("firecracker".to_string()));

        // WHEN: select_provider_with_preferences es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_provider_with_preferences(&job, &providers);

        // THEN: Debe retornar Docker (fallback a estrategia FastestStartup)
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        let selected_provider = providers
            .iter()
            .find(|p| p.provider_id == selected_id)
            .unwrap();
        // Docker tiene fastest startup (5s vs 30s), así que debería ser seleccionado
        assert_eq!(selected_provider.provider_type, ProviderType::Docker);
    }

    #[test]
    fn test_select_provider_matches_by_provider_type() {
        // GIVEN: Job con preferred_provider="kubernetes"
        let providers = create_test_providers();
        let job = create_test_job(Some("kubernetes".to_string()));

        // WHEN: select_provider_with_preferences es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_provider_with_preferences(&job, &providers);

        // THEN: Debe retornar Kubernetes provider
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        let selected_provider = providers
            .iter()
            .find(|p| p.provider_id == selected_id)
            .unwrap();
        assert_eq!(selected_provider.provider_type, ProviderType::Kubernetes);
    }

    #[test]
    fn test_select_provider_applies_strategy_when_no_preference() {
        // GIVEN: Job sin preferred_provider
        let providers = create_test_providers();
        let job = create_test_job(None);

        // WHEN: select_provider_with_preferences es llamado con estrategia FastestStartup
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_provider_with_preferences(&job, &providers);

        // THEN: Debe seleccionar provider con menor estimated_startup_time (Docker)
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        let selected_provider = providers
            .iter()
            .find(|p| p.provider_id == selected_id)
            .unwrap();
        assert_eq!(selected_provider.provider_type, ProviderType::Docker);
    }

    #[test]
    fn test_select_worker_filters_by_required_labels() {
        // GIVEN: Workers con diferentes labels
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
        worker1.mark_connecting().unwrap();
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
        worker2.mark_connecting().unwrap();
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
        worker3.mark_connecting().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        // GIVEN: Job que requiere label "environment=production"
        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        let job =
            create_test_job_with_labels_and_annotations(None, required_labels, HashMap::new());

        // WHEN: select_worker es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        // THEN: Solo worker1 y worker3 deberían ser elegibles (ambos tienen environment=production)
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert!(selected_id == *worker1.id() || selected_id == *worker3.id());
    }

    #[test]
    fn test_select_worker_filters_by_required_annotations() {
        // GIVEN: Workers con diferentes annotations
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
        worker1.mark_connecting().unwrap();
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
        worker2.mark_connecting().unwrap();
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
        worker3.mark_connecting().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        // GIVEN: Job que requiere annotation "team=platform"
        let mut required_annotations = HashMap::new();
        required_annotations.insert("team".to_string(), "platform".to_string());
        let job =
            create_test_job_with_labels_and_annotations(None, HashMap::new(), required_annotations);

        // WHEN: select_worker es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        // THEN: Solo worker1 y worker3 deberían ser elegibles (ambos tienen team=platform)
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert!(selected_id == *worker1.id() || selected_id == *worker3.id());
    }

    #[test]
    fn test_select_worker_filters_by_multiple_required_labels() {
        // GIVEN: Workers con diferentes combinaciones de labels
        let mut spec1 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec1
            .labels
            .insert("environment".to_string(), "production".to_string());
        spec1
            .labels
            .insert("zone".to_string(), "us-east-1".to_string());
        let handle1 = WorkerHandle::new(
            spec1.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker1 = Worker::new(handle1, spec1);
        worker1.mark_connecting().unwrap();
        worker1.mark_ready().unwrap();

        let mut spec2 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50052".to_string(),
        );
        spec2
            .labels
            .insert("environment".to_string(), "production".to_string());
        spec2
            .labels
            .insert("zone".to_string(), "us-west-2".to_string());
        let handle2 = WorkerHandle::new(
            spec2.worker_id.clone(),
            "container-2".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker2 = Worker::new(handle2, spec2);
        worker2.mark_connecting().unwrap();
        worker2.mark_ready().unwrap();

        let mut spec3 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50053".to_string(),
        );
        spec3
            .labels
            .insert("environment".to_string(), "staging".to_string());
        let handle3 = WorkerHandle::new(
            spec3.worker_id.clone(),
            "container-3".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker3 = Worker::new(handle3, spec3);
        worker3.mark_connecting().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        // GIVEN: Job que requiere múltiples labels
        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        required_labels.insert("zone".to_string(), "us-east-1".to_string());
        let job =
            create_test_job_with_labels_and_annotations(None, required_labels, HashMap::new());

        // WHEN: select_worker es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        // THEN: Solo worker1 debería ser elegible (único con ambos labels)
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert_eq!(selected_id, *worker1.id());
    }

    #[test]
    fn test_select_worker_filters_by_labels_and_annotations_combined() {
        // GIVEN: Workers con labels y annotations
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
        worker1.mark_connecting().unwrap();
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
        worker2.mark_connecting().unwrap();
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
        worker3.mark_connecting().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        // GIVEN: Job que requiere tanto labels como annotations
        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        let mut required_annotations = HashMap::new();
        required_annotations.insert("team".to_string(), "platform".to_string());
        let job = create_test_job_with_labels_and_annotations(
            None,
            required_labels,
            required_annotations,
        );

        // WHEN: select_worker es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        // THEN: Solo worker1 debería ser elegible (único con ambos criterios)
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert_eq!(selected_id, *worker1.id());
    }

    #[test]
    fn test_select_worker_returns_none_when_no_workers_match_labels() {
        // GIVEN: Workers sin labels requeridos
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
        worker1.mark_connecting().unwrap();
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
        worker2.mark_connecting().unwrap();
        worker2.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone()];

        // GIVEN: Job que requiere label "environment=production" (que ningún worker tiene)
        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        let job =
            create_test_job_with_labels_and_annotations(None, required_labels, HashMap::new());

        // WHEN: select_worker es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        // THEN: No se debe seleccionar ningún worker
        assert!(selected.is_none());
    }

    #[test]
    fn test_select_worker_filters_by_labels_and_preferred_provider() {
        // GIVEN: Workers de diferentes providers
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
            ProviderType::Kubernetes,
            ProviderId::new(),
        );
        let mut worker1 = Worker::new(handle1, spec1);
        worker1.mark_connecting().unwrap();
        worker1.mark_ready().unwrap();

        let mut spec2 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50052".to_string(),
        );
        spec2
            .labels
            .insert("environment".to_string(), "production".to_string());
        let handle2 = WorkerHandle::new(
            spec2.worker_id.clone(),
            "container-2".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker2 = Worker::new(handle2, spec2);
        worker2.mark_connecting().unwrap();
        worker2.mark_ready().unwrap();

        let mut spec3 = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50053".to_string(),
        );
        spec3
            .labels
            .insert("environment".to_string(), "staging".to_string());
        let handle3 = WorkerHandle::new(
            spec3.worker_id.clone(),
            "container-3".to_string(),
            ProviderType::Kubernetes,
            ProviderId::new(),
        );
        let mut worker3 = Worker::new(handle3, spec3);
        worker3.mark_connecting().unwrap();
        worker3.mark_ready().unwrap();

        let workers = vec![worker1.clone(), worker2.clone(), worker3.clone()];

        // GIVEN: Job con preferred_provider="k8s" y required_labels
        let mut required_labels = HashMap::new();
        required_labels.insert("environment".to_string(), "production".to_string());
        let job = create_test_job_with_labels_and_annotations(
            Some("k8s".to_string()),
            required_labels,
            HashMap::new(),
        );

        // WHEN: select_worker es llamado
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let selected = scheduler.select_worker(&job, &workers);

        // THEN: Solo worker1 debería ser elegible (Kubernetes + environment=production)
        assert!(selected.is_some());
        let selected_id = selected.unwrap();
        assert_eq!(selected_id, *worker1.id());
    }
}
