//! JobRepository Extension Trait
//!
//! Extended repository methods for the Job bounded context.
//! These methods are specific to the application layer and support
//! use cases like timeout monitoring, bulk operations, etc.

use chrono::{DateTime, Utc};
use hodei_server_domain::Result;
use hodei_server_domain::jobs::Job;
use hodei_server_domain::shared_kernel::JobState;

/// Extension trait for JobRepository providing additional query methods.
///
/// This trait provides methods that are specific to the application layer
/// and are not part of the domain repository interface.
#[async_trait::async_trait]
pub trait JobRepositoryExt {
    /// Finds all jobs in a specific state that were created before the given timestamp.
    ///
    /// This method is used by the timeout monitor to find jobs that have been
    /// stuck in a particular state for too long.
    ///
    /// # Arguments
    ///
    /// * `state` - The job state to filter by
    /// * `older_than` - Only return jobs created before this timestamp
    ///
    /// # Returns
    ///
    /// A list of jobs matching the criteria, ordered by creation time (oldest first)
    async fn find_by_state_older_than(
        &self,
        state: JobState,
        older_than: DateTime<Utc>,
    ) -> Result<Vec<Job>>;

    /// Finds jobs in ASSIGNED state older than the given timestamp.
    async fn find_stuck_assigned_jobs(&self, older_than: DateTime<Utc>) -> Result<Vec<Job>> {
        self.find_by_state_older_than(JobState::Assigned, older_than)
            .await
    }

    /// Finds jobs in SCHEDULED state older than the given timestamp.
    async fn find_stuck_scheduled_jobs(&self, older_than: DateTime<Utc>) -> Result<Vec<Job>> {
        self.find_by_state_older_than(JobState::Scheduled, older_than)
            .await
    }

    /// Finds jobs in any intermediate state (Pending, Assigned, Scheduled, Running)
    /// that are older than the given timestamp.
    async fn find_stuck_jobs(&self, older_than: DateTime<Utc>) -> Result<Vec<Job>> {
        let mut stuck_jobs = Vec::new();

        // Check each intermediate state
        for state in [
            JobState::Pending,
            JobState::Assigned,
            JobState::Scheduled,
            JobState::Running,
        ] {
            let mut jobs = self.find_by_state_older_than(state, older_than).await?;
            stuck_jobs.append(&mut jobs);
        }

        // Sort by created_at to process oldest first
        stuck_jobs.sort_by_key(|job| *job.created_at());

        Ok(stuck_jobs)
    }

    /// Counts jobs in each state for monitoring purposes.
    async fn count_by_state(&self) -> Result<std::collections::HashMap<String, usize>>;
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use hodei_server_domain::jobs::JobSpec;
    use hodei_server_domain::shared_kernel::JobId;
    use std::sync::{Arc, Mutex};

    /// Mock implementation of JobRepositoryExt for testing
    #[derive(Clone, Default)]
    pub struct MockJobRepositoryExt {
        jobs: Arc<Mutex<Vec<Job>>>,
    }

    impl MockJobRepositoryExt {
        pub fn new() -> Self {
            Self {
                jobs: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub fn with_jobs(mut self, jobs: Vec<Job>) -> Self {
            self.jobs = Arc::new(Mutex::new(jobs));
            self
        }

        pub fn add_job(&self, job: Job) {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.push(job);
        }
    }

    #[async_trait::async_trait]
    impl JobRepositoryExt for MockJobRepositoryExt {
        async fn find_by_state_older_than(
            &self,
            state: JobState,
            older_than: DateTime<Utc>,
        ) -> Result<Vec<Job>> {
            let jobs = self.jobs.lock().unwrap();
            Ok(jobs
                .iter()
                .filter(|job| *job.state() == state && *job.created_at() < older_than)
                .cloned()
                .collect())
        }

        async fn count_by_state(&self) -> Result<std::collections::HashMap<String, usize>> {
            let jobs = self.jobs.lock().unwrap();
            let mut counts = std::collections::HashMap::new();
            for job in jobs.iter() {
                // Use string representation as key since JobState doesn't implement Hash
                let state_str = job.state().to_string();
                *counts.entry(state_str).or_insert(0) += 1;
            }
            Ok(counts)
        }
    }
}
