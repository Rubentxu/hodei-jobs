//! NotifyingSagaRepository Wrapper
//!
//! This module provides a decorator wrapper around SagaRepository that sends
//! in-memory signals when sagas are saved or updated, enabling reactive
//! saga processing without polling.
//!
//! # Architecture
//!
//! ```text
//! [ API / gRPC ]
//!       │
//!       ▼
//! [ NotifyingSagaRepository ]
//!       │
//!       ├──(1. Transacción SQL)──► [ PostgreSQL ] (Source of Truth)
//!       │
//!       └──(2. Señal Memoria)────► [ Tokio Channel ] ──► [ ReactiveSagaProcessor ]
//!                                                             │
//!       ┌─────────────────────────────────────────────────────┘
//!       ▼
//! [ Ejecución Inmediata (0-5ms) ]
//! ```

use async_trait::async_trait;
use hodei_server_domain::saga::{
    SagaContext, SagaId, SagaRepository as SagaRepositoryTrait, SagaState, SagaStepData,
    SagaStepId, SagaStepState, SagaType,
};
use hodei_server_domain::shared_kernel::DomainError;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error};

/// Errors from NotifyingSagaRepository
#[derive(Debug, Error)]
pub enum NotifyingSagaRepositoryError {
    #[error("Inner repository error: {0}")]
    Inner(#[from] DomainError),

    #[error("Signal channel closed")]
    ChannelClosed,
}

/// The NotifyingSagaRepository decorator
///
/// This wrapper around any SagaRepository implementation adds signal emission
/// for reactive saga processing. It maintains the same API as the inner repository
/// while adding notification capabilities.
pub struct NotifyingSagaRepository<R> {
    /// The wrapped repository
    inner: R,
    /// Channel for sending saga notifications
    signal_tx: mpsc::UnboundedSender<SagaId>,
    /// Metrics
    metrics: Arc<NotifyingRepositoryMetrics>,
}

impl<R> NotifyingSagaRepository<R> {
    /// Create a new NotifyingSagaRepository
    pub fn new(
        inner: R,
        signal_tx: mpsc::UnboundedSender<SagaId>,
        metrics: Arc<NotifyingRepositoryMetrics>,
    ) -> Self {
        Self {
            inner,
            signal_tx,
            metrics,
        }
    }

    /// Emit a notification for saga processing
    fn notify(&self, saga_id: &SagaId) {
        self.metrics.record_notification();

        if let Err(e) = self.signal_tx.send(saga_id.clone()) {
            error!(saga_id = %saga_id, error = %e, "Failed to send saga notification");
        } else {
            debug!(saga_id = %saga_id, "Saga notification sent");
        }
    }
}

#[async_trait]
impl<R: SagaRepositoryTrait<Error = DomainError> + Send + Sync> SagaRepositoryTrait
    for NotifyingSagaRepository<R>
{
    type Error = NotifyingSagaRepositoryError;

    async fn save(&self, context: &SagaContext) -> Result<(), Self::Error> {
        // 1. Persist in DB (Source of Truth)
        self.inner.save(context).await?;

        // 2. Signal the processor (Fast Path)
        self.notify(&context.saga_id);

        Ok(())
    }

    /// EPIC-43: Create saga if it doesn't exist (idempotent)
    async fn create_if_not_exists(&self, context: &SagaContext) -> Result<bool, Self::Error> {
        let created = self.inner.create_if_not_exists(context).await?;
        if created {
            self.notify(&context.saga_id);
        }
        Ok(created)
    }

    async fn find_by_id(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
        self.inner.find_by_id(saga_id).await.map_err(|e| e.into())
    }

    async fn find_by_type(&self, saga_type: SagaType) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner
            .find_by_type(saga_type)
            .await
            .map_err(|e| e.into())
    }

    async fn find_by_state(&self, state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner.find_by_state(state).await.map_err(|e| e.into())
    }

    async fn find_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner
            .find_by_correlation_id(correlation_id)
            .await
            .map_err(|e| e.into())
    }

    async fn update_state(
        &self,
        saga_id: &SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error> {
        // 1. Update in DB
        self.inner
            .update_state(saga_id, state, error_message)
            .await?;

        // 2. Signal the processor (Fast Path)
        self.notify(saga_id);

        Ok(())
    }

    async fn mark_compensating(&self, saga_id: &SagaId) -> Result<(), Self::Error> {
        // 1. Update in DB
        self.inner.mark_compensating(saga_id).await?;

        // 2. Signal the processor
        self.notify(saga_id);

        Ok(())
    }

    async fn delete(&self, saga_id: &SagaId) -> Result<bool, Self::Error> {
        self.inner.delete(saga_id).await.map_err(|e| e.into())
    }

    // ============ Step Operations ============

    async fn save_step(&self, step: &SagaStepData) -> Result<(), Self::Error> {
        self.inner.save_step(step).await.map_err(|e| e.into())
    }

    async fn find_step_by_id(
        &self,
        step_id: &SagaStepId,
    ) -> Result<Option<SagaStepData>, Self::Error> {
        self.inner
            .find_step_by_id(step_id)
            .await
            .map_err(|e| e.into())
    }

    async fn find_steps_by_saga_id(
        &self,
        saga_id: &SagaId,
    ) -> Result<Vec<SagaStepData>, Self::Error> {
        self.inner
            .find_steps_by_saga_id(saga_id)
            .await
            .map_err(|e| e.into())
    }

    async fn update_step_state(
        &self,
        step_id: &SagaStepId,
        state: SagaStepState,
        output: Option<serde_json::Value>,
    ) -> Result<(), Self::Error> {
        self.inner
            .update_step_state(step_id, state, output)
            .await
            .map_err(|e| e.into())
    }

    async fn update_step_compensation(
        &self,
        step_id: &SagaStepId,
        compensation_data: serde_json::Value,
    ) -> Result<(), Self::Error> {
        self.inner
            .update_step_compensation(step_id, compensation_data)
            .await
            .map_err(|e| e.into())
    }

    async fn count_active(&self) -> Result<u64, Self::Error> {
        self.inner.count_active().await.map_err(|e| e.into())
    }

    async fn count_by_type_and_state(
        &self,
        saga_type: SagaType,
        state: SagaState,
    ) -> Result<u64, Self::Error> {
        self.inner
            .count_by_type_and_state(saga_type, state)
            .await
            .map_err(|e| e.into())
    }

    async fn avg_duration(&self) -> Result<Option<std::time::Duration>, Self::Error> {
        self.inner.avg_duration().await.map_err(|e| e.into())
    }

    async fn cleanup_completed(&self, older_than: std::time::Duration) -> Result<u64, Self::Error> {
        self.inner
            .cleanup_completed(older_than)
            .await
            .map_err(|e| e.into())
    }

    async fn claim_pending_sagas(
        &self,
        limit: u64,
        instance_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        self.inner
            .claim_pending_sagas(limit, instance_id)
            .await
            .map_err(|e| e.into())
    }

    async fn save_with_tx(
        &self,
        tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
        context: &SagaContext,
    ) -> Result<(), Self::Error> {
        self.inner
            .save_with_tx(tx, context)
            .await
            .map_err(|e| e.into())
    }

    async fn update_state_with_tx(
        &self,
        tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
        saga_id: &SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error> {
        self.inner
            .update_state_with_tx(tx, saga_id, state, error_message)
            .await
            .map_err(NotifyingSagaRepositoryError::Inner)?;

        // Notify best-effort (note: transaction might not be committed yet)
        self.notify(saga_id);
        Ok(())
    }

    async fn save_step_with_tx(
        &self,
        tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
        step: &SagaStepData,
    ) -> Result<(), Self::Error> {
        self.inner
            .save_step_with_tx(tx, step)
            .await
            .map_err(|e| e.into())
    }

    async fn update_step_state_with_tx(
        &self,
        tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
        step_id: &SagaStepId,
        state: SagaStepState,
        output: Option<serde_json::Value>,
    ) -> Result<(), Self::Error> {
        self.inner
            .update_step_state_with_tx(tx, step_id, state, output)
            .await
            .map_err(|e| e.into())
    }

    async fn update_step_compensation_with_tx(
        &self,
        tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
        step_id: &SagaStepId,
        compensation_data: serde_json::Value,
    ) -> Result<(), Self::Error> {
        self.inner
            .update_step_compensation_with_tx(tx, step_id, compensation_data)
            .await
            .map_err(|e| e.into())
    }
}

/// Metrics for NotifyingSagaRepository
#[derive(Clone, Default)]
pub struct NotifyingRepositoryMetrics {
    notifications_sent: Arc<std::sync::atomic::AtomicU64>,
    notification_errors: Arc<std::sync::atomic::AtomicU64>,
    saves_total: Arc<std::sync::atomic::AtomicU64>,
    updates_total: Arc<std::sync::atomic::AtomicU64>,
}

impl NotifyingRepositoryMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self {
            notifications_sent: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            notification_errors: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            saves_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            updates_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Record a notification sent
    pub fn record_notification(&self) {
        self.notifications_sent
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a notification error
    pub fn record_error(&self) {
        self.notification_errors
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a save operation
    pub fn record_save(&self) {
        self.saves_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record an update operation
    pub fn record_update(&self) {
        self.updates_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get notification count
    pub fn notifications_sent(&self) -> u64 {
        self.notifications_sent
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get error count
    pub fn notification_errors(&self) -> u64 {
        self.notification_errors
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Builder for NotifyingSagaRepository
pub struct NotifyingSagaRepositoryBuilder<R> {
    inner: Option<R>,
    metrics: Option<Arc<NotifyingRepositoryMetrics>>,
}

impl<R> NotifyingSagaRepositoryBuilder<R> {
    /// Create a new builder
    pub fn new(inner: R) -> Self {
        Self {
            inner: Some(inner),
            metrics: None,
        }
    }

    /// Set metrics
    pub fn with_metrics(mut self, metrics: Arc<NotifyingRepositoryMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Build the NotifyingSagaRepository
    pub fn build(self, signal_tx: mpsc::UnboundedSender<SagaId>) -> NotifyingSagaRepository<R> {
        NotifyingSagaRepository::new(
            self.inner.expect("inner repository required"),
            signal_tx,
            self.metrics
                .unwrap_or_else(|| Arc::new(NotifyingRepositoryMetrics::default())),
        )
    }
}

/// Create a signal channel for saga notifications
///
/// Returns (sender, receiver) where:
/// - sender should be passed to NotifyingSagaRepository
/// - receiver should be passed to ReactiveSagaProcessor
#[allow(dead_code)]
pub fn create_signal_channel(
    _capacity: usize,
) -> (
    mpsc::UnboundedSender<SagaId>,
    mpsc::UnboundedReceiver<SagaId>,
) {
    mpsc::unbounded_channel::<SagaId>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::{SagaContext, SagaId, SagaType};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// Mock repository for testing
    struct MockSagaRepository;

    #[async_trait::async_trait]
    impl SagaRepositoryTrait for MockSagaRepository {
        type Error = DomainError;

        async fn save(&self, _: &SagaContext) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn create_if_not_exists(&self, _: &SagaContext) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn find_by_id(&self, _: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
            Ok(None)
        }

        async fn find_by_type(&self, _: SagaType) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(vec![])
        }

        async fn find_by_state(&self, _: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(vec![])
        }

        async fn find_by_correlation_id(&self, _: &str) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(vec![])
        }

        async fn update_state(
            &self,
            _: &SagaId,
            _: SagaState,
            _: Option<String>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn mark_compensating(&self, _: &SagaId) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn delete(&self, _: &SagaId) -> Result<bool, Self::Error> {
            Ok(false)
        }

        async fn save_step(&self, _: &SagaStepData) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn find_step_by_id(
            &self,
            _: &SagaStepId,
        ) -> Result<Option<SagaStepData>, Self::Error> {
            Ok(None)
        }

        async fn find_steps_by_saga_id(
            &self,
            _: &SagaId,
        ) -> Result<Vec<SagaStepData>, Self::Error> {
            Ok(vec![])
        }

        async fn update_step_state(
            &self,
            _: &SagaStepId,
            _: SagaStepState,
            _: Option<serde_json::Value>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn update_step_compensation(
            &self,
            _: &SagaStepId,
            _: serde_json::Value,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn count_active(&self) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn count_by_type_and_state(
            &self,
            _: SagaType,
            _: SagaState,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn avg_duration(&self) -> Result<Option<std::time::Duration>, Self::Error> {
            Ok(None)
        }

        async fn cleanup_completed(&self, _: std::time::Duration) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn claim_pending_sagas(
            &self,
            _: u64,
            _: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(vec![])
        }

        async fn save_with_tx(
            &self,
            _tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
            _context: &SagaContext,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn update_state_with_tx(
            &self,
            _tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
            _saga_id: &SagaId,
            _state: SagaState,
            _error: Option<String>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn save_step_with_tx(
            &self,
            _tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
            _step: &SagaStepData,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn update_step_state_with_tx(
            &self,
            _tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
            _step_id: &SagaStepId,
            _state: SagaStepState,
            _output: Option<serde_json::Value>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn update_step_compensation_with_tx(
            &self,
            _tx: &mut hodei_server_domain::transaction::PgTransaction<'_>,
            _step_id: &SagaStepId,
            _compensation: serde_json::Value,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn create_test_saga_context() -> SagaContext {
        SagaContext::new(
            SagaId(uuid::Uuid::new_v4()),
            SagaType::Execution,
            None,
            None,
        )
    }

    #[tokio::test]
    async fn test_notify_on_save() {
        let (tx, mut rx) = mpsc::unbounded_channel::<SagaId>();
        let metrics = Arc::new(NotifyingRepositoryMetrics::default());
        let repo = NotifyingSagaRepository::new(MockSagaRepository, tx, metrics.clone());

        let saga = create_test_saga_context();
        repo.save(&saga).await.unwrap();

        // Verify notification was sent
        let notification = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .unwrap();
        assert!(notification.is_some());
        assert_eq!(notification.unwrap(), saga.saga_id);

        assert_eq!(metrics.notifications_sent(), 1);
    }

    #[tokio::test]
    async fn test_notify_on_update_state() {
        let (tx, mut rx) = mpsc::unbounded_channel::<SagaId>();
        let metrics = Arc::new(NotifyingRepositoryMetrics::default());
        let repo = NotifyingSagaRepository::new(MockSagaRepository, tx, metrics.clone());

        let saga_id = SagaId(uuid::Uuid::new_v4());
        repo.update_state(&saga_id, SagaState::InProgress, None)
            .await
            .unwrap();

        // Verify notification was sent
        let notification = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .unwrap();
        assert!(notification.is_some());
        assert_eq!(notification.unwrap(), saga_id);
    }

    #[tokio::test]
    async fn test_notify_on_mark_compensating() {
        let (tx, mut rx) = mpsc::unbounded_channel::<SagaId>();
        let metrics = Arc::new(NotifyingRepositoryMetrics::default());
        let repo = NotifyingSagaRepository::new(MockSagaRepository, tx, metrics.clone());

        let saga_id = SagaId(uuid::Uuid::new_v4());
        repo.mark_compensating(&saga_id).await.unwrap();

        // Verify notification was sent
        let notification = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .unwrap();
        assert!(notification.is_some());
        assert_eq!(notification.unwrap(), saga_id);
    }

    #[tokio::test]
    async fn test_no_notify_on_query_operations() {
        let (tx, mut rx) = mpsc::unbounded_channel::<SagaId>();
        let metrics = Arc::new(NotifyingRepositoryMetrics::default());
        let repo = NotifyingSagaRepository::new(MockSagaRepository, tx, metrics.clone());

        // Query operations should not trigger notifications
        repo.find_by_id(&SagaId(uuid::Uuid::new_v4()))
            .await
            .unwrap();
        repo.find_by_type(SagaType::Execution).await.unwrap();
        repo.find_by_state(SagaState::Pending).await.unwrap();
        repo.find_by_correlation_id("test").await.unwrap();

        // Verify no notifications were sent
        assert_eq!(metrics.notifications_sent(), 0);

        // Verify no notification in channel
        let notification =
            tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(notification.is_err()); // Timeout = no notification
    }
}
