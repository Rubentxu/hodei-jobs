//! Stuck Saga Detector
//!
//! EPIC-46 GAP-12: Implements detection of "zombie" sagas that are blocked.
//!
//! This module provides functionality to detect sagas that have been stuck
//! in a non-terminal state for too long, typically due to:
//! - Network failures
//! - Worker crashes
//! - Deadlocks
//! - Unhandled errors

use super::{SagaId, SagaRepository, SagaState};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for stuck saga detection
#[derive(Debug, Clone)]
pub struct StuckSagaDetectorConfig {
    /// Maximum time a saga can be in progress before being considered stuck
    pub stuck_threshold: Duration,
    /// Maximum time a saga can be compensating before being considered stuck
    pub compensating_threshold: Duration,
    /// How often to run the detection check
    pub check_interval: Duration,
    /// Whether to automatically recover stuck sagas
    pub auto_recover: bool,
}

impl Default for StuckSagaDetectorConfig {
    fn default() -> Self {
        Self {
            stuck_threshold: Duration::from_secs(600), // 10 minutes
            compensating_threshold: Duration::from_secs(300), // 5 minutes
            check_interval: Duration::from_secs(60),   // 1 minute
            auto_recover: false,
        }
    }
}

/// Result of stuck saga detection
#[derive(Debug, Clone)]
pub struct StuckSagaInfo {
    /// The ID of the stuck saga
    pub saga_id: SagaId,
    /// Current state of the saga
    pub state: SagaState,
    /// How long the saga has been stuck
    pub stuck_duration: Duration,
    /// Reason for being considered stuck
    pub reason: String,
}

/// Trait for detecting stuck sagas
///
/// EPIC-46 GAP-12: Provides mechanism to detect sagas that are blocked.
#[async_trait::async_trait]
pub trait StuckSagaDetector: Send + Sync {
    /// Detects sagas that have been stuck for longer than the threshold
    ///
    /// # Returns
    /// A list of saga IDs that are considered stuck
    async fn detect_stuck_sagas(&self) -> Result<Vec<StuckSagaInfo>, StuckSagaDetectorError>;

    /// Attempts to recover a stuck saga
    ///
    /// Recovery may involve:
    /// - Retrying the current step
    /// - Starting compensation
    /// - Marking as failed
    async fn recover_saga(&self, saga_id: &SagaId) -> Result<(), StuckSagaDetectorError>;

    /// Gets the detector configuration
    fn config(&self) -> &StuckSagaDetectorConfig;
}

/// Errors from stuck saga detection
#[derive(Debug, thiserror::Error)]
pub enum StuckSagaDetectorError {
    #[error("Repository error: {message}")]
    RepositoryError { message: String },

    #[error("Saga not found: {saga_id}")]
    SagaNotFound { saga_id: SagaId },

    #[error("Recovery failed for saga {saga_id}: {message}")]
    RecoveryFailed { saga_id: SagaId, message: String },
}

/// In-memory implementation of StuckSagaDetector for testing
#[derive(Debug)]
pub struct InMemoryStuckSagaDetector<R: SagaRepository> {
    repository: std::sync::Arc<R>,
    config: StuckSagaDetectorConfig,
}

impl<R: SagaRepository> InMemoryStuckSagaDetector<R> {
    /// Creates a new detector with the given repository and configuration
    pub fn new(repository: std::sync::Arc<R>, config: StuckSagaDetectorConfig) -> Self {
        Self { repository, config }
    }

    /// Creates a new detector with default configuration
    pub fn with_defaults(repository: std::sync::Arc<R>) -> Self {
        Self::new(repository, StuckSagaDetectorConfig::default())
    }
}

#[async_trait::async_trait]
impl<R: SagaRepository + Send + Sync> StuckSagaDetector for InMemoryStuckSagaDetector<R> {
    async fn detect_stuck_sagas(&self) -> Result<Vec<StuckSagaInfo>, StuckSagaDetectorError> {
        let mut stuck_sagas = Vec::new();
        let now = chrono::Utc::now();

        // Query pending sagas that are stuck
        // Query pending sagas that are stuck
        // Combine sagas in Pending, InProgress, and Compensating states
        let mut pending_sagas = Vec::new();

        let in_progress = self
            .repository
            .find_by_state(SagaState::InProgress)
            .await
            .map_err(|e| StuckSagaDetectorError::RepositoryError {
                message: format!("{:?}", e),
            })?;
        pending_sagas.extend(in_progress);

        let compensating = self
            .repository
            .find_by_state(SagaState::Compensating)
            .await
            .map_err(|e| StuckSagaDetectorError::RepositoryError {
                message: format!("{:?}", e),
            })?;
        pending_sagas.extend(compensating);

        let pending = self
            .repository
            .find_by_state(SagaState::Pending)
            .await
            .map_err(|e| StuckSagaDetectorError::RepositoryError {
                message: format!("{:?}", e),
            })?;
        pending_sagas.extend(pending);

        for saga_ctx in pending_sagas {
            let elapsed = now - saga_ctx.started_at;
            let elapsed_duration = Duration::from_secs(elapsed.num_seconds().max(0) as u64);

            let threshold = match saga_ctx.state {
                SagaState::InProgress | SagaState::Pending => self.config.stuck_threshold,
                SagaState::Compensating => self.config.compensating_threshold,
                _ => continue, // Terminal states are not stuck
            };

            if elapsed_duration > threshold {
                let reason = match saga_ctx.state {
                    SagaState::InProgress => {
                        format!(
                            "Saga has been in progress for {:?} (threshold: {:?})",
                            elapsed_duration, threshold
                        )
                    }
                    SagaState::Compensating => {
                        format!(
                            "Saga has been compensating for {:?} (threshold: {:?})",
                            elapsed_duration, threshold
                        )
                    }
                    SagaState::Pending => {
                        format!(
                            "Saga has been pending for {:?} (threshold: {:?})",
                            elapsed_duration, threshold
                        )
                    }
                    _ => continue,
                };

                warn!(
                    saga_id = %saga_ctx.saga_id,
                    state = %saga_ctx.state,
                    elapsed = ?elapsed_duration,
                    "Detected stuck saga"
                );

                stuck_sagas.push(StuckSagaInfo {
                    saga_id: saga_ctx.saga_id.clone(),
                    state: saga_ctx.state,
                    stuck_duration: elapsed_duration,
                    reason,
                });
            }
        }

        if !stuck_sagas.is_empty() {
            info!(count = stuck_sagas.len(), "Found stuck sagas");
        } else {
            debug!("No stuck sagas detected");
        }

        Ok(stuck_sagas)
    }

    async fn recover_saga(&self, saga_id: &SagaId) -> Result<(), StuckSagaDetectorError> {
        let saga_ctx = self
            .repository
            .find_by_id(saga_id)
            .await
            .map_err(|e| StuckSagaDetectorError::RepositoryError {
                message: format!("{:?}", e),
            })?
            .ok_or_else(|| StuckSagaDetectorError::SagaNotFound {
                saga_id: saga_id.clone(),
            })?;

        info!(
            saga_id = %saga_id,
            current_state = %saga_ctx.state,
            "Attempting to recover stuck saga"
        );

        // Mark saga as failed if it can't progress
        if saga_ctx.state == SagaState::InProgress || saga_ctx.state == SagaState::Pending {
            let mut updated_ctx = saga_ctx.clone();
            updated_ctx.state = SagaState::Failed;
            updated_ctx.error_message = Some("Marked as failed by stuck saga detector".to_string());

            self.repository
                .update_state(saga_id, SagaState::Failed, updated_ctx.error_message)
                .await
                .map_err(|e| StuckSagaDetectorError::RecoveryFailed {
                    saga_id: saga_id.clone(),
                    message: format!("{:?}", e),
                })?;

            info!(saga_id = %saga_id, "Stuck saga marked as failed");
        } else if saga_ctx.state == SagaState::Compensating {
            // For compensating sagas, mark as failed (compensation timeout)
            let mut updated_ctx = saga_ctx.clone();
            updated_ctx.state = SagaState::Failed;
            updated_ctx.error_message = Some(
                "Compensation timed out - marked as failed by stuck saga detector".to_string(),
            );

            self.repository
                .update_state(saga_id, SagaState::Failed, updated_ctx.error_message)
                .await
                .map_err(|e| StuckSagaDetectorError::RecoveryFailed {
                    saga_id: saga_id.clone(),
                    message: format!("{:?}", e),
                })?;

            info!(
                saga_id = %saga_id,
                "Stuck compensating saga marked as failed"
            );
        }

        Ok(())
    }

    fn config(&self) -> &StuckSagaDetectorConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StuckSagaDetectorConfig::default();
        assert_eq!(config.stuck_threshold, Duration::from_secs(600));
        assert_eq!(config.compensating_threshold, Duration::from_secs(300));
        assert_eq!(config.check_interval, Duration::from_secs(60));
        assert!(!config.auto_recover);
    }

    #[test]
    fn test_stuck_saga_info() {
        let info = StuckSagaInfo {
            saga_id: SagaId::new(),
            state: SagaState::InProgress,
            stuck_duration: Duration::from_secs(700),
            reason: "Test reason".to_string(),
        };

        assert!(info.stuck_duration > Duration::from_secs(600));
    }
}
