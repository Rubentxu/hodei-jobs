//! Scheduling Domain Events
//!
//! Domain events for the scheduling bounded context.
//! These events represent significant occurrences in the scheduling process
//! and are used for observability, auditing, and side effects.
//!
//! ## Design Principles
//! - Events are immutable facts that have happened
//! - Events contain all necessary context for processing
//! - Events follow the builder pattern for construction
//! - Events are serializable for storage and transmission

use crate::jobs::JobPreferences;
use crate::shared_kernel::{JobId, ProviderId, WorkerId};
use crate::workers::ProviderType;
use chrono::{DateTime, Utc};

/// Events that occur during the scheduling process
///
/// These events replace the `tracing::debug!` calls in the domain layer,
/// achieving purity of domain by decoupling scheduling logic from observability.
///
/// ## Connascence Reduction
/// - **Before**: Connascence of Algorithm - logging mixed with scheduling logic
/// - **After**: Connascence of Type - events as pure domain objects
#[derive(Debug, Clone, PartialEq)]
pub enum SchedulingEvent {
    /// Scheduling process has started for a job
    SchedulingStarted {
        job_id: JobId,
        job_preferences: JobPreferences,
        available_workers_count: usize,
        available_providers_count: usize,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// Worker filtering has started
    WorkerFilteringStarted {
        job_id: JobId,
        preferred_provider: Option<String>,
        required_labels_count: usize,
        required_annotations_count: usize,
        total_workers: usize,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// Worker filtering completed
    WorkerFilteringCompleted {
        job_id: JobId,
        total_workers: usize,
        eligible_workers_count: usize,
        filter_reason: FilterReason,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// Worker selection has started
    WorkerSelectionStarted {
        job_id: JobId,
        eligible_workers_count: usize,
        selection_strategy: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// A worker has been selected
    WorkerSelected {
        job_id: JobId,
        worker_id: WorkerId,
        provider_type: ProviderType,
        selection_strategy: String,
        elapsed_ms: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// No eligible worker could be selected
    WorkerSelectionFailed {
        job_id: JobId,
        reason: SelectionFailureReason,
        eligible_workers_count: usize,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// Provider selection has started
    ProviderSelectionStarted {
        job_id: JobId,
        preferred_provider: Option<String>,
        available_providers_count: usize,
        selection_strategy: String,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// A provider has been selected for provisioning
    ProviderSelected {
        job_id: JobId,
        provider_id: ProviderId,
        provider_type: ProviderType,
        selection_strategy: String,
        effective_cost: f64,
        effective_startup_ms: u64,
        elapsed_ms: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// No provider could be selected
    ProviderSelectionFailed {
        job_id: JobId,
        reason: ProviderSelectionFailureReason,
        available_providers_count: usize,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// A scheduling decision has been made
    SchedulingDecisionMade {
        job_id: JobId,
        decision: SchedulingDecisionKind,
        reason: String,
        elapsed_ms: u64,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },

    /// Scheduling process failed with an error
    SchedulingFailed {
        job_id: JobId,
        error: String,
        stage: SchedulingStage,
        occurred_at: DateTime<Utc>,
        correlation_id: Option<String>,
    },
}

/// Reason why workers were filtered out
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterReason {
    /// All workers matched
    AllMatched,
    /// No workers available
    NoWorkersAvailable,
    /// Provider type mismatch
    ProviderTypeMismatch,
    /// Labels mismatch
    LabelsMismatch,
    /// Annotations mismatch,
    AnnotationsMismatch,
    /// Combined filter criteria
    CombinedCriteria,
}

/// Reason why worker selection failed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionFailureReason {
    /// No workers were eligible after filtering
    NoEligibleWorkers,
    /// All eligible workers were busy
    AllWorkersBusy,
    /// Selection strategy failed
    StrategyFailed,
    /// Internal error during selection
    InternalError { message: String },
}

/// Reason why provider selection failed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderSelectionFailureReason {
    /// No providers available
    NoProvidersAvailable,
    /// Preferred provider not available
    PreferredProviderUnavailable { provider_name: String },
    /// All providers at capacity
    AllProvidersAtCapacity,
    /// Provider health too low
    ProvidersUnhealthy,
    /// Selection strategy failed
    StrategyFailed,
}

/// Kind of scheduling decision made
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulingDecisionKind {
    /// Assigned to existing worker
    AssignToWorker,
    /// Provisioned new worker
    ProvisionWorker,
    /// Enqueued for later processing
    Enqueue,
    /// Rejected due to resource constraints
    Reject,
}

/// Stage at which scheduling failed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulingStage {
    /// Failed during worker filtering
    WorkerFiltering,
    /// Failed during worker selection
    WorkerSelection,
    /// Failed during provider selection
    ProviderSelection,
    /// Failed during decision making
    DecisionMaking,
    /// Failed due to queue limit
    QueueLimit,
}

// =============================================================================
// Builder Implementations
// =============================================================================

/// Builder for SchedulingStarted event
#[derive(Debug, Clone)]
pub struct SchedulingStartedBuilder {
    job_id: JobId,
    job_preferences: JobPreferences,
    available_workers_count: usize,
    available_providers_count: usize,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
}

impl SchedulingStartedBuilder {
    /// Create a new builder
    pub fn new(
        job_id: JobId,
        job_preferences: JobPreferences,
        available_workers_count: usize,
        available_providers_count: usize,
    ) -> Self {
        Self {
            job_id,
            job_preferences,
            available_workers_count,
            available_providers_count,
            occurred_at: Utc::now(),
            correlation_id: None,
        }
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Build the event
    pub fn build(self) -> SchedulingEvent {
        SchedulingEvent::SchedulingStarted {
            job_id: self.job_id,
            job_preferences: self.job_preferences,
            available_workers_count: self.available_workers_count,
            available_providers_count: self.available_providers_count,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
        }
    }
}

/// Builder for WorkerFilteringCompleted event
#[derive(Debug, Clone)]
pub struct WorkerFilteringCompletedBuilder {
    job_id: JobId,
    total_workers: usize,
    eligible_workers_count: usize,
    filter_reason: FilterReason,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
}

impl WorkerFilteringCompletedBuilder {
    /// Create a new builder
    pub fn new(job_id: JobId, total_workers: usize, eligible_workers_count: usize) -> Self {
        let filter_reason = if eligible_workers_count == 0 {
            if total_workers == 0 {
                FilterReason::NoWorkersAvailable
            } else {
                FilterReason::CombinedCriteria
            }
        } else if eligible_workers_count == total_workers {
            FilterReason::AllMatched
        } else {
            FilterReason::CombinedCriteria
        };

        Self {
            job_id,
            total_workers,
            eligible_workers_count,
            filter_reason,
            occurred_at: Utc::now(),
            correlation_id: None,
        }
    }

    /// Set filter reason explicitly
    pub fn with_filter_reason(mut self, filter_reason: FilterReason) -> Self {
        self.filter_reason = filter_reason;
        self
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Build the event
    pub fn build(self) -> SchedulingEvent {
        SchedulingEvent::WorkerFilteringCompleted {
            job_id: self.job_id,
            total_workers: self.total_workers,
            eligible_workers_count: self.eligible_workers_count,
            filter_reason: self.filter_reason,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
        }
    }
}

/// Builder for WorkerSelected event
#[derive(Debug, Clone)]
pub struct WorkerSelectedBuilder {
    job_id: JobId,
    worker_id: WorkerId,
    provider_type: ProviderType,
    selection_strategy: String,
    elapsed_ms: u64,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
}

impl WorkerSelectedBuilder {
    /// Create a new builder
    pub fn new(
        job_id: JobId,
        worker_id: WorkerId,
        provider_type: ProviderType,
        selection_strategy: String,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            job_id,
            worker_id,
            provider_type,
            selection_strategy,
            elapsed_ms,
            occurred_at: Utc::now(),
            correlation_id: None,
        }
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Build the event
    pub fn build(self) -> SchedulingEvent {
        SchedulingEvent::WorkerSelected {
            job_id: self.job_id,
            worker_id: self.worker_id,
            provider_type: self.provider_type,
            selection_strategy: self.selection_strategy,
            elapsed_ms: self.elapsed_ms,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
        }
    }
}

/// Builder for ProviderSelected event
#[derive(Debug, Clone)]
pub struct ProviderSelectedBuilder {
    job_id: JobId,
    provider_id: ProviderId,
    provider_type: ProviderType,
    selection_strategy: String,
    effective_cost: f64,
    effective_startup_ms: u64,
    elapsed_ms: u64,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
}

impl ProviderSelectedBuilder {
    /// Create a new builder
    pub fn new(
        job_id: JobId,
        provider_id: ProviderId,
        provider_type: ProviderType,
        selection_strategy: String,
        effective_cost: f64,
        effective_startup_ms: u64,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            job_id,
            provider_id,
            provider_type,
            selection_strategy,
            effective_cost,
            effective_startup_ms,
            elapsed_ms,
            occurred_at: Utc::now(),
            correlation_id: None,
        }
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Build the event
    pub fn build(self) -> SchedulingEvent {
        SchedulingEvent::ProviderSelected {
            job_id: self.job_id,
            provider_id: self.provider_id,
            provider_type: self.provider_type,
            selection_strategy: self.selection_strategy,
            effective_cost: self.effective_cost,
            effective_startup_ms: self.effective_startup_ms,
            elapsed_ms: self.elapsed_ms,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
        }
    }
}

/// Builder for SchedulingDecisionMade event
#[derive(Debug, Clone)]
pub struct SchedulingDecisionMadeBuilder {
    job_id: JobId,
    decision: SchedulingDecisionKind,
    reason: String,
    elapsed_ms: u64,
    occurred_at: DateTime<Utc>,
    correlation_id: Option<String>,
}

impl SchedulingDecisionMadeBuilder {
    /// Create a new builder
    pub fn new(
        job_id: JobId,
        decision: SchedulingDecisionKind,
        reason: String,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            job_id,
            decision,
            reason,
            elapsed_ms,
            occurred_at: Utc::now(),
            correlation_id: None,
        }
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Build the event
    pub fn build(self) -> SchedulingEvent {
        SchedulingEvent::SchedulingDecisionMade {
            job_id: self.job_id,
            decision: self.decision,
            reason: self.reason,
            elapsed_ms: self.elapsed_ms,
            occurred_at: self.occurred_at,
            correlation_id: self.correlation_id,
        }
    }
}

// =============================================================================
// Event Extension Trait
// =============================================================================

/// Extension trait for emitting scheduling events
///
/// This trait allows any component to emit scheduling events
/// without depending on a specific event publisher implementation.
pub trait SchedulingEventEmitter {
    /// Get a reference to the collected events
    fn events(&self) -> &Vec<SchedulingEvent>;

    /// Get a mutable reference to collect new events
    fn events_mut(&mut self) -> &mut Vec<SchedulingEvent>;

    /// Emit a scheduling event
    fn emit(&mut self, event: SchedulingEvent) {
        self.events_mut().push(event);
    }

    /// Create a builder for SchedulingStarted
    fn emit_scheduling_started(
        &mut self,
        job_id: JobId,
        job_preferences: JobPreferences,
        available_workers_count: usize,
        available_providers_count: usize,
    ) {
        let event = SchedulingStartedBuilder::new(
            job_id,
            job_preferences,
            available_workers_count,
            available_providers_count,
        )
        .build();
        self.emit(event);
    }

    /// Create a builder for WorkerFilteringCompleted
    fn emit_worker_filtering_completed(
        &mut self,
        job_id: JobId,
        total_workers: usize,
        eligible_workers_count: usize,
    ) {
        let event =
            WorkerFilteringCompletedBuilder::new(job_id, total_workers, eligible_workers_count)
                .build();
        self.emit(event);
    }

    /// Create a builder for WorkerSelected
    fn emit_worker_selected(
        &mut self,
        job_id: JobId,
        worker_id: WorkerId,
        provider_type: ProviderType,
        selection_strategy: String,
        elapsed_ms: u64,
    ) {
        let event = WorkerSelectedBuilder::new(
            job_id,
            worker_id,
            provider_type,
            selection_strategy,
            elapsed_ms,
        )
        .build();
        self.emit(event);
    }

    /// Create a builder for ProviderSelected
    fn emit_provider_selected(
        &mut self,
        job_id: JobId,
        provider_id: ProviderId,
        provider_type: ProviderType,
        selection_strategy: String,
        effective_cost: f64,
        effective_startup_ms: u64,
        elapsed_ms: u64,
    ) {
        let event = ProviderSelectedBuilder::new(
            job_id,
            provider_id,
            provider_type,
            selection_strategy,
            effective_cost,
            effective_startup_ms,
            elapsed_ms,
        )
        .build();
        self.emit(event);
    }

    /// Create a builder for SchedulingDecisionMade
    fn emit_scheduling_decision_made(
        &mut self,
        job_id: JobId,
        decision: SchedulingDecisionKind,
        reason: String,
        elapsed_ms: u64,
    ) {
        let event =
            SchedulingDecisionMadeBuilder::new(job_id, decision, reason, elapsed_ms).build();
        self.emit(event);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobPreferences;
    use crate::shared_kernel::{JobId, ProviderId, WorkerId};
    use crate::workers::ProviderType;
    use std::collections::HashMap;

    #[test]
    fn test_scheduling_started_builder() {
        let job_id = JobId::new();
        let preferences = JobPreferences::default();

        let event = SchedulingStartedBuilder::new(job_id.clone(), preferences.clone(), 5, 3)
            .with_correlation_id("test-corr".to_string())
            .build();

        match event {
            SchedulingEvent::SchedulingStarted {
                job_id: actual_job_id,
                job_preferences: actual_prefs,
                available_workers_count,
                available_providers_count,
                occurred_at: _,
                correlation_id,
            } => {
                assert_eq!(actual_job_id, job_id);
                assert_eq!(actual_prefs, preferences);
                assert_eq!(available_workers_count, 5);
                assert_eq!(available_providers_count, 3);
                assert_eq!(correlation_id, Some("test-corr".to_string()));
            }
            _ => panic!("Expected SchedulingStarted event"),
        }
    }

    #[test]
    fn test_worker_filtering_completed_builder_all_matched() {
        let job_id = JobId::new();

        let event = WorkerFilteringCompletedBuilder::new(job_id.clone(), 10, 10).build();

        match event {
            SchedulingEvent::WorkerFilteringCompleted {
                job_id: _,
                total_workers: 10,
                eligible_workers_count: 10,
                filter_reason: FilterReason::AllMatched,
                occurred_at: _,
                correlation_id: _,
            } => {
                // Expected
            }
            _ => panic!("Expected WorkerFilteringCompleted with AllMatched"),
        }
    }

    #[test]
    fn test_worker_filtering_completed_builder_no_workers() {
        let job_id = JobId::new();

        let event = WorkerFilteringCompletedBuilder::new(job_id.clone(), 0, 0).build();

        match event {
            SchedulingEvent::WorkerFilteringCompleted {
                job_id: _,
                total_workers: 0,
                eligible_workers_count: 0,
                filter_reason: FilterReason::NoWorkersAvailable,
                occurred_at: _,
                correlation_id: _,
            } => {
                // Expected
            }
            _ => panic!("Expected WorkerFilteringCompleted with NoWorkersAvailable"),
        }
    }

    #[test]
    fn test_worker_selected_builder() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        let event = WorkerSelectedBuilder::new(
            job_id.clone(),
            worker_id.clone(),
            ProviderType::Kubernetes,
            "least_loaded".to_string(),
            5,
        )
        .with_correlation_id("test-corr".to_string())
        .build();

        match event {
            SchedulingEvent::WorkerSelected {
                job_id: _,
                worker_id: _,
                provider_type,
                selection_strategy,
                elapsed_ms,
                occurred_at: _,
                correlation_id,
            } => {
                assert_eq!(provider_type, ProviderType::Kubernetes);
                assert_eq!(selection_strategy, "least_loaded");
                assert_eq!(elapsed_ms, 5);
                assert_eq!(correlation_id, Some("test-corr".to_string()));
            }
            _ => panic!("Expected WorkerSelected event"),
        }
    }

    #[test]
    fn test_provider_selected_builder() {
        let job_id = JobId::new();
        let provider_id = ProviderId::new();

        let event = ProviderSelectedBuilder::new(
            job_id.clone(),
            provider_id.clone(),
            ProviderType::Kubernetes,
            "fastest_startup".to_string(),
            0.5,
            5000,
            3,
        )
        .with_correlation_id("test-corr".to_string())
        .build();

        match event {
            SchedulingEvent::ProviderSelected {
                job_id: _,
                provider_id: _,
                provider_type,
                selection_strategy,
                effective_cost,
                effective_startup_ms,
                elapsed_ms,
                occurred_at: _,
                correlation_id,
            } => {
                assert_eq!(provider_type, ProviderType::Kubernetes);
                assert_eq!(selection_strategy, "fastest_startup");
                assert!((effective_cost - 0.5).abs() < 0.001);
                assert_eq!(effective_startup_ms, 5000);
                assert_eq!(elapsed_ms, 3);
                assert_eq!(correlation_id, Some("test-corr".to_string()));
            }
            _ => panic!("Expected ProviderSelected event"),
        }
    }

    #[test]
    fn test_scheduling_decision_made_builder() {
        let job_id = JobId::new();

        let event = SchedulingDecisionMadeBuilder::new(
            job_id.clone(),
            SchedulingDecisionKind::ProvisionWorker,
            "No workers available".to_string(),
            10,
        )
        .with_correlation_id("test-corr".to_string())
        .build();

        match event {
            SchedulingEvent::SchedulingDecisionMade {
                job_id: _,
                decision,
                reason,
                elapsed_ms,
                occurred_at: _,
                correlation_id,
            } => {
                assert_eq!(decision, SchedulingDecisionKind::ProvisionWorker);
                assert_eq!(reason, "No workers available");
                assert_eq!(elapsed_ms, 10);
                assert_eq!(correlation_id, Some("test-corr".to_string()));
            }
            _ => panic!("Expected SchedulingDecisionMade event"),
        }
    }

    #[test]
    fn test_scheduling_event_emitter_trait() {
        struct TestEmitter {
            events: Vec<SchedulingEvent>,
        }

        impl SchedulingEventEmitter for TestEmitter {
            fn events(&self) -> &Vec<SchedulingEvent> {
                &self.events
            }

            fn events_mut(&mut self) -> &mut Vec<SchedulingEvent> {
                &mut self.events
            }
        }

        let mut emitter = TestEmitter { events: Vec::new() };
        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        emitter.emit_worker_selected(
            job_id.clone(),
            worker_id.clone(),
            ProviderType::Docker,
            "first_available".to_string(),
            2,
        );

        assert_eq!(emitter.events().len(), 1);

        match &emitter.events()[0] {
            SchedulingEvent::WorkerSelected {
                worker_id: actual_worker_id,
                ..
            } => {
                assert_eq!(actual_worker_id, &worker_id);
            }
            _ => panic!("Expected WorkerSelected event"),
        }
    }
}
