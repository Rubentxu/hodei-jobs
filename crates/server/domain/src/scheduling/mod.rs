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
    /// Prefer existing workers over provisioning
    pub prefer_existing_workers: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            worker_strategy: WorkerSelectionStrategy::LeastLoaded,
            provider_strategy: ProviderSelectionStrategy::FastestStartup,
            max_queue_depth: 100,
            scale_up_load_threshold: 0.8,
            prefer_existing_workers: true,
        }
    }
}

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

        let available_count = workers
            .iter()
            .filter(|w| w.state().can_accept_jobs())
            .count();
        tracing::debug!(
            job_id = %job.id,
            total_workers = workers.len(),
            available_workers = available_count,
            strategy = ?self.config.worker_strategy,
            "Evaluating workers for job scheduling"
        );

        // Log worker details
        for (idx, worker) in workers.iter().enumerate() {
            tracing::debug!(
                job_id = %job.id,
                worker_id = %worker.id(),
                worker_state = ?worker.state(),
                can_accept = worker.state().can_accept_jobs(),
                "Checking worker {} for job",
                idx + 1
            );
        }

        let result = match self.config.worker_strategy {
            WorkerSelectionStrategy::FirstAvailable => {
                FirstAvailableWorkerSelector.select_worker(job, workers)
            }
            WorkerSelectionStrategy::LeastLoaded => {
                LeastLoadedWorkerSelector.select_worker(job, workers)
            }
            WorkerSelectionStrategy::RoundRobin => {
                let available: Vec<_> = workers
                    .iter()
                    .filter(|w| w.state().can_accept_jobs())
                    .collect();

                if available.is_empty() {
                    tracing::debug!(job_id = %job.id, "No available workers for RoundRobin strategy");
                    return None;
                }

                let index = self.round_robin_worker_index.fetch_add(1, Ordering::SeqCst);
                let worker = available[index % available.len()];
                Some(worker.id().clone())
            }
            WorkerSelectionStrategy::MostCapacity => {
                // For now, same as LeastLoaded (could be enhanced with resource metrics)
                LeastLoadedWorkerSelector.select_worker(job, workers)
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

                let result = workers
                    .iter()
                    .filter(|w| w.state().can_accept_jobs())
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
                        workers.iter().find(|w| w.state().can_accept_jobs())
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

        // Try to assign to existing worker first if preferred
        if self.config.prefer_existing_workers {
            if let Some(worker_id) = self.select_worker(&context.job, &context.available_workers) {
                return Ok(SchedulingDecision::AssignToWorker { job_id, worker_id });
            }
        }

        // Try to provision new worker
        if let Some(provider_id) = self.select_provider(&context.job, &context.available_providers)
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
            reason: "No workers or providers available".to_string(),
        })
    }

    fn strategy_name(&self) -> &str {
        "smart_scheduler"
    }
}
