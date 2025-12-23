//! Service to manage worker health and heartbeat calculations.

use crate::workers::Worker;
use chrono::Utc;
use std::time::Duration;

/// Newtype for Heartbeat Age to avoid confusion with other Durations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeartbeatAge(pub Duration);

impl HeartbeatAge {
    pub fn new(duration: Duration) -> Self {
        Self(duration)
    }

    pub fn as_duration(&self) -> Duration {
        self.0
    }
}

/// Status of the worker based on its heartbeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerHealthStatus {
    Healthy,
    /// Worker is technically alive but heartbeat is older than expected (grace period).
    Degraded(HeartbeatAge),
    /// Worker is considered dead/disconnected.
    Dead(HeartbeatAge),
}

impl WorkerHealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// Service responsible for assessing worker health.
#[derive(Debug, Clone)]
pub struct WorkerHealthService {
    heartbeat_timeout: Duration,
}

impl WorkerHealthService {
    /// Creates a builder for WorkerHealthService.
    pub fn builder() -> WorkerHealthServiceBuilder {
        WorkerHealthServiceBuilder::default()
    }

    /// Calculates the age of the last heartbeat.
    pub fn calculate_heartbeat_age(&self, worker: &Worker) -> HeartbeatAge {
        let now = Utc::now();
        // signed_duration_since returns chrono::Duration.
        // We convert to std::time::Duration, defaulting to 0 if negative (clock skew protection)
        let delta = now.signed_duration_since(worker.last_heartbeat());
        let duration = delta.to_std().unwrap_or(Duration::ZERO);
        HeartbeatAge(duration)
    }

    /// Assesses the health of a worker.
    pub fn assess_worker_health(&self, worker: &Worker) -> WorkerHealthStatus {
        let age = self.calculate_heartbeat_age(worker);

        if age.0 < self.heartbeat_timeout {
            WorkerHealthStatus::Healthy
        } else if age.0 < self.heartbeat_timeout.saturating_mul(2) {
            // Example logic for Degraded: 1x to 2x timeout
            WorkerHealthStatus::Degraded(age)
        } else {
            WorkerHealthStatus::Dead(age)
        }
    }

    /// Convenience method to check if a worker is healthy.
    pub fn is_healthy(&self, worker: &Worker) -> bool {
        self.assess_worker_health(worker).is_healthy()
    }

    /// Returns current heartbeat timeout configuration
    pub fn timeout(&self) -> Duration {
        self.heartbeat_timeout
    }
}

/// Builder for WorkerHealthService.
#[derive(Default)]
pub struct WorkerHealthServiceBuilder {
    heartbeat_timeout: Option<Duration>,
}

impl WorkerHealthServiceBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = Some(timeout);
        self
    }

    pub fn build(self) -> WorkerHealthService {
        WorkerHealthService {
            heartbeat_timeout: self.heartbeat_timeout.unwrap_or(Duration::from_secs(30)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::{ProviderId, WorkerId};
    use crate::workers::{ProviderType, Worker, WorkerHandle, WorkerSpec};
    use chrono::Utc;

    fn create_test_worker(last_heartbeat_age_secs: u64) -> Worker {
        let now = Utc::now();
        let last_heartbeat = now - chrono::Duration::seconds(last_heartbeat_age_secs as i64);

        let handle = WorkerHandle::new(
            WorkerId::new(),
            "test-resource".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let spec = WorkerSpec::new("img".to_string(), "addr".to_string());

        Worker::from_database(
            handle,
            spec,
            crate::shared_kernel::WorkerState::Ready,
            None,
            Some(last_heartbeat),
            now,
            now,
        )
    }

    #[test]
    fn test_healthy_worker() {
        let service = WorkerHealthService::builder()
            .with_heartbeat_timeout(Duration::from_secs(30))
            .build();

        let worker = create_test_worker(10); // 10s age
        let status = service.assess_worker_health(&worker);

        assert!(matches!(status, WorkerHealthStatus::Healthy));
        assert!(status.is_healthy());
    }

    #[test]
    fn test_degraded_worker() {
        let service = WorkerHealthService::builder()
            .with_heartbeat_timeout(Duration::from_secs(30))
            .build();

        let worker = create_test_worker(45); // 45s age (between 30 and 60)
        let status = service.assess_worker_health(&worker);

        match status {
            WorkerHealthStatus::Degraded(age) => assert_eq!(age.0.as_secs(), 45),
            _ => panic!("Expected Degraded status"),
        }
        assert!(!status.is_healthy());
    }

    #[test]
    fn test_dead_worker() {
        let service = WorkerHealthService::builder()
            .with_heartbeat_timeout(Duration::from_secs(30))
            .build();

        let worker = create_test_worker(70); // 70s age (> 60)
        let status = service.assess_worker_health(&worker);

        match status {
            WorkerHealthStatus::Dead(age) => assert_eq!(age.0.as_secs(), 70),
            _ => panic!("Expected Dead status"),
        }
        assert!(!status.is_healthy());
    }
}
