//! Lifecycle Metrics and Observability
//!
//! Comprehensive metrics for monitoring Jobs, Workers, Events, and system health.
//! Designed for Prometheus integration with Grafana dashboards.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Metrics for job lifecycle tracking
#[derive(Debug)]
pub struct JobLifecycleMetrics {
    /// Total jobs created
    jobs_created_total: AtomicU64,
    /// Total jobs completed successfully
    jobs_completed_total: AtomicU64,
    /// Total jobs failed
    jobs_failed_total: AtomicU64,
    /// Total jobs timed out
    jobs_timed_out_total: AtomicU64,
    /// Total jobs cancelled
    jobs_cancelled_total: AtomicU64,
    /// Jobs currently in each state
    jobs_in_pending: AtomicU64,
    jobs_in_assigned: AtomicU64,
    jobs_in_scheduled: AtomicU64,
    jobs_in_running: AtomicU64,
    jobs_in_succeeded: AtomicU64,
    jobs_in_failed: AtomicU64,
}

impl JobLifecycleMetrics {
    /// Create a new instance
    pub fn new() -> Self {
        Self {
            jobs_created_total: AtomicU64::new(0),
            jobs_completed_total: AtomicU64::new(0),
            jobs_failed_total: AtomicU64::new(0),
            jobs_timed_out_total: AtomicU64::new(0),
            jobs_cancelled_total: AtomicU64::new(0),
            jobs_in_pending: AtomicU64::new(0),
            jobs_in_assigned: AtomicU64::new(0),
            jobs_in_scheduled: AtomicU64::new(0),
            jobs_in_running: AtomicU64::new(0),
            jobs_in_succeeded: AtomicU64::new(0),
            jobs_in_failed: AtomicU64::new(0),
        }
    }

    /// Record job created
    pub fn record_job_created(&self) {
        self.jobs_created_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record job completed successfully
    pub fn record_job_completed(&self) {
        self.jobs_completed_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record job failed
    pub fn record_job_failed(&self) {
        self.jobs_failed_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record job timed out
    pub fn record_job_timed_out(&self) {
        self.jobs_timed_out_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record job cancelled
    pub fn record_job_cancelled(&self) {
        self.jobs_cancelled_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment count for a specific state
    pub fn increment_state(&self, state: &str) {
        if state == "pending" {
            self.jobs_in_pending.fetch_add(1, Ordering::Relaxed);
        } else if state == "assigned" {
            self.jobs_in_assigned.fetch_add(1, Ordering::Relaxed);
        } else if state == "scheduled" {
            self.jobs_in_scheduled.fetch_add(1, Ordering::Relaxed);
        } else if state == "running" {
            self.jobs_in_running.fetch_add(1, Ordering::Relaxed);
        } else if state == "succeeded" {
            self.jobs_in_succeeded.fetch_add(1, Ordering::Relaxed);
        } else if state == "failed" {
            self.jobs_in_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Decrement count for a specific state
    pub fn decrement_state(&self, state: &str) {
        if state == "pending" {
            self.jobs_in_pending.fetch_sub(1, Ordering::Relaxed);
        } else if state == "assigned" {
            self.jobs_in_assigned.fetch_sub(1, Ordering::Relaxed);
        } else if state == "scheduled" {
            self.jobs_in_scheduled.fetch_sub(1, Ordering::Relaxed);
        } else if state == "running" {
            self.jobs_in_running.fetch_sub(1, Ordering::Relaxed);
        } else if state == "succeeded" {
            self.jobs_in_succeeded.fetch_sub(1, Ordering::Relaxed);
        } else if state == "failed" {
            self.jobs_in_failed.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Getters for metric values
    pub fn jobs_created_total(&self) -> u64 {
        self.jobs_created_total.load(Ordering::Relaxed)
    }
    pub fn jobs_completed_total(&self) -> u64 {
        self.jobs_completed_total.load(Ordering::Relaxed)
    }
    pub fn jobs_failed_total(&self) -> u64 {
        self.jobs_failed_total.load(Ordering::Relaxed)
    }
    pub fn jobs_timed_out_total(&self) -> u64 {
        self.jobs_timed_out_total.load(Ordering::Relaxed)
    }
    pub fn jobs_cancelled_total(&self) -> u64 {
        self.jobs_cancelled_total.load(Ordering::Relaxed)
    }
    pub fn jobs_in_pending(&self) -> u64 {
        self.jobs_in_pending.load(Ordering::Relaxed)
    }
    pub fn jobs_in_assigned(&self) -> u64 {
        self.jobs_in_assigned.load(Ordering::Relaxed)
    }
    pub fn jobs_in_scheduled(&self) -> u64 {
        self.jobs_in_scheduled.load(Ordering::Relaxed)
    }
    pub fn jobs_in_running(&self) -> u64 {
        self.jobs_in_running.load(Ordering::Relaxed)
    }
    pub fn jobs_in_succeeded(&self) -> u64 {
        self.jobs_in_succeeded.load(Ordering::Relaxed)
    }
    pub fn jobs_in_failed(&self) -> u64 {
        self.jobs_in_failed.load(Ordering::Relaxed)
    }
}

impl Clone for JobLifecycleMetrics {
    fn clone(&self) -> Self {
        Self {
            jobs_created_total: AtomicU64::new(self.jobs_created_total.load(Ordering::Relaxed)),
            jobs_completed_total: AtomicU64::new(self.jobs_completed_total.load(Ordering::Relaxed)),
            jobs_failed_total: AtomicU64::new(self.jobs_failed_total.load(Ordering::Relaxed)),
            jobs_timed_out_total: AtomicU64::new(self.jobs_timed_out_total.load(Ordering::Relaxed)),
            jobs_cancelled_total: AtomicU64::new(self.jobs_cancelled_total.load(Ordering::Relaxed)),
            jobs_in_pending: AtomicU64::new(self.jobs_in_pending.load(Ordering::Relaxed)),
            jobs_in_assigned: AtomicU64::new(self.jobs_in_assigned.load(Ordering::Relaxed)),
            jobs_in_scheduled: AtomicU64::new(self.jobs_in_scheduled.load(Ordering::Relaxed)),
            jobs_in_running: AtomicU64::new(self.jobs_in_running.load(Ordering::Relaxed)),
            jobs_in_succeeded: AtomicU64::new(self.jobs_in_succeeded.load(Ordering::Relaxed)),
            jobs_in_failed: AtomicU64::new(self.jobs_in_failed.load(Ordering::Relaxed)),
        }
    }
}

/// Metrics for worker lifecycle tracking
#[derive(Debug)]
pub struct WorkerLifecycleMetrics {
    /// Total workers created
    workers_created_total: AtomicU64,
    /// Total workers registered
    workers_registered_total: AtomicU64,
    /// Total workers terminated
    workers_terminated_total: AtomicU64,
    /// Total workers failed
    workers_failed_total: AtomicU64,
    /// Workers currently in each state
    workers_in_creating: AtomicU64,
    workers_in_connecting: AtomicU64,
    workers_in_ready: AtomicU64,
    workers_in_busy: AtomicU64,
    workers_in_draining: AtomicU64,
    workers_in_terminating: AtomicU64,
    workers_in_terminated: AtomicU64,
}

impl WorkerLifecycleMetrics {
    /// Create a new instance
    pub fn new() -> Self {
        Self {
            workers_created_total: AtomicU64::new(0),
            workers_registered_total: AtomicU64::new(0),
            workers_terminated_total: AtomicU64::new(0),
            workers_failed_total: AtomicU64::new(0),
            workers_in_creating: AtomicU64::new(0),
            workers_in_connecting: AtomicU64::new(0),
            workers_in_ready: AtomicU64::new(0),
            workers_in_busy: AtomicU64::new(0),
            workers_in_draining: AtomicU64::new(0),
            workers_in_terminating: AtomicU64::new(0),
            workers_in_terminated: AtomicU64::new(0),
        }
    }

    /// Record worker created
    pub fn record_worker_created(&self) {
        self.workers_created_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record worker registered
    pub fn record_worker_registered(&self) {
        self.workers_registered_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record worker terminated
    pub fn record_worker_terminated(&self) {
        self.workers_terminated_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record worker failed
    pub fn record_worker_failed(&self) {
        self.workers_failed_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment count for a specific state
    pub fn increment_state(&self, state: &str) {
        if state == "creating" {
            self.workers_in_creating.fetch_add(1, Ordering::Relaxed);
        } else if state == "connecting" {
            self.workers_in_connecting.fetch_add(1, Ordering::Relaxed);
        } else if state == "ready" {
            self.workers_in_ready.fetch_add(1, Ordering::Relaxed);
        } else if state == "busy" {
            self.workers_in_busy.fetch_add(1, Ordering::Relaxed);
        } else if state == "draining" {
            self.workers_in_draining.fetch_add(1, Ordering::Relaxed);
        } else if state == "terminating" {
            self.workers_in_terminating.fetch_add(1, Ordering::Relaxed);
        } else if state == "terminated" {
            self.workers_in_terminated.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Decrement count for a specific state
    pub fn decrement_state(&self, state: &str) {
        if state == "creating" {
            self.workers_in_creating.fetch_sub(1, Ordering::Relaxed);
        } else if state == "connecting" {
            self.workers_in_connecting.fetch_sub(1, Ordering::Relaxed);
        } else if state == "ready" {
            self.workers_in_ready.fetch_sub(1, Ordering::Relaxed);
        } else if state == "busy" {
            self.workers_in_busy.fetch_sub(1, Ordering::Relaxed);
        } else if state == "draining" {
            self.workers_in_draining.fetch_sub(1, Ordering::Relaxed);
        } else if state == "terminating" {
            self.workers_in_terminating.fetch_sub(1, Ordering::Relaxed);
        } else if state == "terminated" {
            self.workers_in_terminated.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Getters
    pub fn workers_created_total(&self) -> u64 {
        self.workers_created_total.load(Ordering::Relaxed)
    }
    pub fn workers_registered_total(&self) -> u64 {
        self.workers_registered_total.load(Ordering::Relaxed)
    }
    pub fn workers_terminated_total(&self) -> u64 {
        self.workers_terminated_total.load(Ordering::Relaxed)
    }
    pub fn workers_failed_total(&self) -> u64 {
        self.workers_failed_total.load(Ordering::Relaxed)
    }
    pub fn workers_in_creating(&self) -> u64 {
        self.workers_in_creating.load(Ordering::Relaxed)
    }
    pub fn workers_in_connecting(&self) -> u64 {
        self.workers_in_connecting.load(Ordering::Relaxed)
    }
    pub fn workers_in_ready(&self) -> u64 {
        self.workers_in_ready.load(Ordering::Relaxed)
    }
    pub fn workers_in_busy(&self) -> u64 {
        self.workers_in_busy.load(Ordering::Relaxed)
    }
    pub fn workers_in_draining(&self) -> u64 {
        self.workers_in_draining.load(Ordering::Relaxed)
    }
    pub fn workers_in_terminating(&self) -> u64 {
        self.workers_in_terminating.load(Ordering::Relaxed)
    }
    pub fn workers_in_terminated(&self) -> u64 {
        self.workers_in_terminated.load(Ordering::Relaxed)
    }
}

impl Clone for WorkerLifecycleMetrics {
    fn clone(&self) -> Self {
        Self {
            workers_created_total: AtomicU64::new(
                self.workers_created_total.load(Ordering::Relaxed),
            ),
            workers_registered_total: AtomicU64::new(
                self.workers_registered_total.load(Ordering::Relaxed),
            ),
            workers_terminated_total: AtomicU64::new(
                self.workers_terminated_total.load(Ordering::Relaxed),
            ),
            workers_failed_total: AtomicU64::new(self.workers_failed_total.load(Ordering::Relaxed)),
            workers_in_creating: AtomicU64::new(self.workers_in_creating.load(Ordering::Relaxed)),
            workers_in_connecting: AtomicU64::new(
                self.workers_in_connecting.load(Ordering::Relaxed),
            ),
            workers_in_ready: AtomicU64::new(self.workers_in_ready.load(Ordering::Relaxed)),
            workers_in_busy: AtomicU64::new(self.workers_in_busy.load(Ordering::Relaxed)),
            workers_in_draining: AtomicU64::new(self.workers_in_draining.load(Ordering::Relaxed)),
            workers_in_terminating: AtomicU64::new(
                self.workers_in_terminating.load(Ordering::Relaxed),
            ),
            workers_in_terminated: AtomicU64::new(
                self.workers_in_terminated.load(Ordering::Relaxed),
            ),
        }
    }
}

/// Metrics for event publishing
#[derive(Debug)]
pub struct EventMetrics {
    /// Total events published
    events_published_total: AtomicU64,
    /// Total events failed to publish
    events_failed_total: AtomicU64,
    /// Events currently in outbox queue
    outbox_queue_size: AtomicU64,
    /// Events in dead letter queue
    dlq_size: AtomicU64,
    /// Event publish duration histogram (buckets for percentiles)
    publish_duration_sum: AtomicU64,
    publish_duration_count: AtomicU64,
}

impl EventMetrics {
    /// Create a new instance
    pub fn new() -> Self {
        Self {
            events_published_total: AtomicU64::new(0),
            events_failed_total: AtomicU64::new(0),
            outbox_queue_size: AtomicU64::new(0),
            dlq_size: AtomicU64::new(0),
            publish_duration_sum: AtomicU64::new(0),
            publish_duration_count: AtomicU64::new(0),
        }
    }

    /// Record event published
    pub fn record_event_published(&self, duration: Duration) {
        self.events_published_total.fetch_add(1, Ordering::Relaxed);
        self.publish_duration_sum
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.publish_duration_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record event publish failure
    pub fn record_event_failed(&self) {
        self.events_failed_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Update outbox queue size
    pub fn set_outbox_queue_size(&self, size: u64) {
        self.outbox_queue_size.store(size, Ordering::Relaxed);
    }

    /// Update DLQ size
    pub fn set_dlq_size(&self, size: u64) {
        self.dlq_size.store(size, Ordering::Relaxed);
    }

    /// Getters
    pub fn events_published_total(&self) -> u64 {
        self.events_published_total.load(Ordering::Relaxed)
    }
    pub fn events_failed_total(&self) -> u64 {
        self.events_failed_total.load(Ordering::Relaxed)
    }
    pub fn outbox_queue_size(&self) -> u64 {
        self.outbox_queue_size.load(Ordering::Relaxed)
    }
    pub fn dlq_size(&self) -> u64 {
        self.dlq_size.load(Ordering::Relaxed)
    }
    pub fn publish_duration_avg_ms(&self) -> u64 {
        let sum = self.publish_duration_sum.load(Ordering::Relaxed);
        let count = self.publish_duration_count.load(Ordering::Relaxed);
        if count > 0 { sum / count } else { 0 }
    }
}

impl Clone for EventMetrics {
    fn clone(&self) -> Self {
        Self {
            events_published_total: AtomicU64::new(
                self.events_published_total.load(Ordering::Relaxed),
            ),
            events_failed_total: AtomicU64::new(self.events_failed_total.load(Ordering::Relaxed)),
            outbox_queue_size: AtomicU64::new(self.outbox_queue_size.load(Ordering::Relaxed)),
            dlq_size: AtomicU64::new(self.dlq_size.load(Ordering::Relaxed)),
            publish_duration_sum: AtomicU64::new(self.publish_duration_sum.load(Ordering::Relaxed)),
            publish_duration_count: AtomicU64::new(
                self.publish_duration_count.load(Ordering::Relaxed),
            ),
        }
    }
}

/// Metrics for dispatch operations
#[derive(Debug)]
pub struct DispatchMetrics {
    /// Total dispatch attempts
    dispatch_attempts_total: AtomicU64,
    /// Total dispatch failures
    dispatch_failures_total: AtomicU64,
    /// Dispatch duration
    dispatch_duration_sum: AtomicU64,
    dispatch_duration_count: AtomicU64,
    /// Active dispatches
    active_dispatches: AtomicU64,
}

impl DispatchMetrics {
    /// Create a new instance
    pub fn new() -> Self {
        Self {
            dispatch_attempts_total: AtomicU64::new(0),
            dispatch_failures_total: AtomicU64::new(0),
            dispatch_duration_sum: AtomicU64::new(0),
            dispatch_duration_count: AtomicU64::new(0),
            active_dispatches: AtomicU64::new(0),
        }
    }

    /// Record dispatch attempt
    pub fn record_dispatch_attempt(&self, duration: Duration) {
        self.dispatch_attempts_total.fetch_add(1, Ordering::Relaxed);
        self.dispatch_duration_sum
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.dispatch_duration_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record dispatch failure
    pub fn record_dispatch_failure(&self) {
        self.dispatch_failures_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment active dispatches
    pub fn increment_active(&self) {
        self.active_dispatches.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active dispatches
    pub fn decrement_active(&self) {
        self.active_dispatches.fetch_sub(1, Ordering::Relaxed);
    }

    /// Getters
    pub fn dispatch_attempts_total(&self) -> u64 {
        self.dispatch_attempts_total.load(Ordering::Relaxed)
    }
    pub fn dispatch_failures_total(&self) -> u64 {
        self.dispatch_failures_total.load(Ordering::Relaxed)
    }
    pub fn dispatch_failures_rate(&self) -> f64 {
        let attempts = self.dispatch_attempts_total.load(Ordering::Relaxed);
        let failures = self.dispatch_failures_total.load(Ordering::Relaxed);
        if attempts > 0 {
            failures as f64 / attempts as f64
        } else {
            0.0
        }
    }
    pub fn active_dispatches(&self) -> u64 {
        self.active_dispatches.load(Ordering::Relaxed)
    }
}

impl Clone for DispatchMetrics {
    fn clone(&self) -> Self {
        Self {
            dispatch_attempts_total: AtomicU64::new(
                self.dispatch_attempts_total.load(Ordering::Relaxed),
            ),
            dispatch_failures_total: AtomicU64::new(
                self.dispatch_failures_total.load(Ordering::Relaxed),
            ),
            dispatch_duration_sum: AtomicU64::new(
                self.dispatch_duration_sum.load(Ordering::Relaxed),
            ),
            dispatch_duration_count: AtomicU64::new(
                self.dispatch_duration_count.load(Ordering::Relaxed),
            ),
            active_dispatches: AtomicU64::new(self.active_dispatches.load(Ordering::Relaxed)),
        }
    }
}

/// Combined lifecycle metrics for easy access
#[derive(Debug)]
pub struct LifecycleMetrics {
    pub jobs: JobLifecycleMetrics,
    pub workers: WorkerLifecycleMetrics,
    pub events: EventMetrics,
    pub dispatch: DispatchMetrics,
}

impl Default for LifecycleMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleMetrics {
    /// Create a new instance with all metric sub-collectors
    pub fn new() -> Self {
        Self {
            jobs: JobLifecycleMetrics::new(),
            workers: WorkerLifecycleMetrics::new(),
            events: EventMetrics::new(),
            dispatch: DispatchMetrics::new(),
        }
    }
}

impl Clone for LifecycleMetrics {
    fn clone(&self) -> Self {
        Self {
            jobs: self.jobs.clone(),
            workers: self.workers.clone(),
            events: self.events.clone(),
            dispatch: self.dispatch.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_metrics_created_increments() {
        let metrics = JobLifecycleMetrics::new();
        metrics.record_job_created();
        assert_eq!(metrics.jobs_created_total(), 1);
        metrics.record_job_created();
        assert_eq!(metrics.jobs_created_total(), 2);
    }

    #[test]
    fn test_job_state_tracking() {
        let metrics = JobLifecycleMetrics::new();
        metrics.increment_state("running");
        metrics.increment_state("pending");
        assert_eq!(metrics.jobs_in_running(), 1);
        assert_eq!(metrics.jobs_in_pending(), 1);

        metrics.decrement_state("running");
        assert_eq!(metrics.jobs_in_running(), 0);
    }

    #[test]
    fn test_worker_metrics() {
        let metrics = WorkerLifecycleMetrics::new();
        metrics.record_worker_created();
        metrics.record_worker_registered();
        assert_eq!(metrics.workers_created_total(), 1);
        assert_eq!(metrics.workers_registered_total(), 1);
    }

    #[test]
    fn test_event_metrics() {
        let metrics = EventMetrics::new();
        metrics.record_event_published(Duration::from_millis(50));
        assert_eq!(metrics.events_published_total(), 1);
        assert_eq!(metrics.publish_duration_avg_ms(), 50);
    }

    #[test]
    fn test_dispatch_metrics() {
        let metrics = DispatchMetrics::new();
        metrics.record_dispatch_attempt(Duration::from_millis(100));
        metrics.record_dispatch_failure();
        assert_eq!(metrics.dispatch_attempts_total(), 1);
        assert_eq!(metrics.dispatch_failures_total(), 1);
        assert!((metrics.dispatch_failures_rate() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_lifecycle_metrics_combined() {
        let metrics = LifecycleMetrics::new();
        metrics.jobs.record_job_created();
        metrics.workers.record_worker_created();
        metrics
            .events
            .record_event_published(Duration::from_millis(10));

        assert_eq!(metrics.jobs.jobs_created_total(), 1);
        assert_eq!(metrics.workers.workers_created_total(), 1);
        assert_eq!(metrics.events.events_published_total(), 1);
    }

    #[test]
    fn test_clone_preserves_values() {
        let original = JobLifecycleMetrics::new();
        original.record_job_created();
        original.record_job_failed();
        original.increment_state("running");

        let cloned = original.clone();

        assert_eq!(cloned.jobs_created_total(), 1);
        assert_eq!(cloned.jobs_failed_total(), 1);
        assert_eq!(cloned.jobs_in_running(), 1);
    }
}
