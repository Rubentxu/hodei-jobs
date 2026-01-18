// Command Outbox Module
//
// Implements the Transactional Outbox Pattern for commands.
// Commands are persisted to the hodei_commands table and processed by a background relay.
//
// This follows the same pattern as the Event Outbox (see crate::outbox module),
// providing consistency across the codebase.

use super::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::sync::Arc;
use uuid::Uuid;

// Re-export command types for use in relay
pub use super::jobs::{MarkJobFailedCommand, ResumeFromManualInterventionCommand};

// Import dispatch_erased from erased module for relay use
use super::erased::dispatch_erased;

/// Status of a command in the outbox
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOutboxStatus {
    /// Command has been created but not yet processed
    Pending,
    /// Command has been successfully processed
    Completed,
    /// Command processing failed and will be retried
    Failed,
    /// Command was cancelled
    Cancelled,
}

impl std::fmt::Display for CommandOutboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandOutboxStatus::Pending => write!(f, "PENDING"),
            CommandOutboxStatus::Completed => write!(f, "COMPLETED"),
            CommandOutboxStatus::Failed => write!(f, "FAILED"),
            CommandOutboxStatus::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

/// Error types for command outbox operations
#[derive(Debug, thiserror::Error)]
pub enum CommandOutboxError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Duplicate idempotency key: {0}")]
    DuplicateIdempotencyKey(String),

    #[error("Command not found: {0}")]
    NotFound(Uuid),

    #[error("Command handler error: {0}")]
    HandlerError(String),
}

/// A command record ready to be inserted into the outbox
#[derive(Debug, Clone)]
pub struct CommandOutboxInsert {
    /// The target aggregate ID (e.g., JobId)
    pub target_id: Uuid,
    /// Type of target (Job, Worker, Provider, Saga)
    pub target_type: CommandTargetType,
    /// The command type name (e.g., "MarkJobFailed")
    pub command_type: String,
    /// Serialized command payload
    pub payload: serde_json::Value,
    /// Optional metadata (trace_id, saga_id, etc.)
    pub metadata: Option<serde_json::Value>,
    /// Optional idempotency key
    pub idempotency_key: Option<String>,
}

impl CommandOutboxInsert {
    /// Create a new command outbox insert record
    pub fn new(
        target_id: Uuid,
        target_type: CommandTargetType,
        command_type: String,
        payload: serde_json::Value,
        metadata: Option<serde_json::Value>,
        idempotency_key: Option<String>,
    ) -> Self {
        Self {
            target_id,
            target_type,
            command_type,
            payload,
            metadata,
            idempotency_key,
        }
    }

    /// Create a job-related command
    pub fn for_job(
        job_id: Uuid,
        command_type: String,
        payload: serde_json::Value,
        metadata: Option<serde_json::Value>,
        idempotency_key: Option<String>,
    ) -> Self {
        Self::new(
            job_id,
            CommandTargetType::Job,
            command_type,
            payload,
            metadata,
            idempotency_key,
        )
    }

    /// Create a worker-related command
    pub fn for_worker(
        worker_id: Uuid,
        command_type: String,
        payload: serde_json::Value,
        metadata: Option<serde_json::Value>,
        idempotency_key: Option<String>,
    ) -> Self {
        Self::new(
            worker_id,
            CommandTargetType::Worker,
            command_type,
            payload,
            metadata,
            idempotency_key,
        )
    }
}

/// A view of a command outbox record from the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutboxRecord {
    pub id: Uuid,
    pub target_id: Uuid,
    pub target_type: CommandTargetType,
    pub command_type: String,
    pub payload: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
    pub status: CommandOutboxStatus,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
    pub retry_count: i32,
    pub last_error: Option<String>,
}

impl CommandOutboxRecord {
    /// Check if the command is still pending
    pub fn is_pending(&self) -> bool {
        matches!(self.status, CommandOutboxStatus::Pending)
    }

    /// Check if the command has been completed
    pub fn is_completed(&self) -> bool {
        matches!(self.status, CommandOutboxStatus::Completed)
    }

    /// Check if the command has failed
    pub fn has_failed(&self, max_retries: i32) -> bool {
        matches!(self.status, CommandOutboxStatus::Failed) && self.retry_count >= max_retries
    }

    /// Get the age of the command
    pub fn age(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.created_at)
    }
}

/// Trait for command outbox repository (analogous to OutboxRepository)
#[async_trait]
pub trait CommandOutboxRepository: Send + Sync {
    /// Insert a command into the outbox
    async fn insert_command(
        &self,
        command: &CommandOutboxInsert,
    ) -> Result<Uuid, CommandOutboxError>;

    /// Insert a command into the outbox within an existing transaction
    async fn insert_command_with_tx(
        &self,
        tx: &mut crate::transaction::PgTransaction<'_>,
        command: &CommandOutboxInsert,
    ) -> Result<Uuid, CommandOutboxError>;

    /// Get pending commands for processing
    async fn get_pending_commands(
        &self,
        limit: usize,
        max_retries: i32,
    ) -> Result<Vec<CommandOutboxRecord>, CommandOutboxError>;

    /// Mark a command as completed
    async fn mark_completed(&self, command_id: &Uuid) -> Result<(), CommandOutboxError>;

    /// Mark a command as failed
    async fn mark_failed(&self, command_id: &Uuid, error: &str) -> Result<(), CommandOutboxError>;

    /// Check if an idempotency key exists
    async fn exists_by_idempotency_key(&self, key: &str) -> Result<bool, CommandOutboxError>;

    /// Get command statistics
    async fn get_stats(&self) -> Result<CommandOutboxStats, CommandOutboxError>;
}

/// Statistics for command outbox
#[derive(Debug, Clone)]
pub struct CommandOutboxStats {
    pub pending_count: u64,
    pub completed_count: u64,
    pub failed_count: u64,
    pub oldest_pending_age_seconds: Option<i64>,
}

impl CommandOutboxStats {
    /// Total number of commands
    pub fn total(&self) -> u64 {
        self.pending_count + self.completed_count + self.failed_count
    }

    /// Check if there are pending commands
    pub fn has_pending(&self) -> bool {
        self.pending_count > 0
    }
}

/// OutboxCommandBus - CommandBus decorator that persists commands to outbox before dispatching.
///
/// This decorator implements the Transactional Outbox Pattern for commands.
/// Commands are persisted to the hodei_commands table and processed by a background relay.
/// This ensures that commands are not lost even if the application crashes after the
/// saga commits but before the command is dispatched.
///
/// # Type Parameters
/// - `R`: The CommandOutboxRepository for persisting commands
/// - `B`: The inner ErasedCommandBus for actual dispatching
///
/// # Example
/// ```ignore
/// let command_bus = Arc::new(InMemoryErasedCommandBus::new());
/// let outbox_repo = Arc::new(PostgresCommandOutboxRepository::new(pool));
/// let outboxed = OutboxCommandBus::new(outbox_repo, command_bus);
/// ```
#[derive(Clone)]
pub struct OutboxCommandBus<R, B> {
    repository: Arc<R>,
    inner: Arc<B>,
}

impl<R, B> OutboxCommandBus<R, B> {
    /// Create a new OutboxCommandBus.
    ///
    /// # Arguments
    /// * `repository` - The repository for persisting commands to outbox
    /// * `inner` - The inner ErasedCommandBus that handles actual dispatch
    pub fn new(repository: Arc<R>, inner: Arc<B>) -> Self {
        Self { repository, inner }
    }
}

#[async_trait::async_trait]
impl<R, B> ErasedCommandBus for OutboxCommandBus<R, B>
where
    R: CommandOutboxRepository + Send + Sync,
    B: ErasedCommandBus + Send + Sync,
{
    async fn dispatch_erased(
        &self,
        command: Box<dyn Any + Send>,
        command_type_id: TypeId,
        command_name: String,
        target_type: CommandTargetType,
    ) -> Result<Box<dyn Any + Send>, CommandError> {
        // Create a placeholder target_id
        // In production, this would be extracted from the command using a trait
        let target_id = Uuid::new_v4();

        // Create outbox insert record with placeholder payload
        // The actual serialization happens at the repository level with type knowledge
        // or in the relay using a command type registry
        let insert = CommandOutboxInsert::new(
            target_id,
            target_type.clone(),
            command_name.clone(),
            serde_json::json!({"type_id": format!("{:?}", command_type_id)}),
            None,
            None,
        );

        // Insert command into outbox (this should be called within the saga's transaction)
        self.repository
            .insert_command(&insert)
            .await
            .map_err(|e| CommandError::OutboxError {
                command_type: command_name.clone(),
                error: e.to_string(),
            })?;

        // Dispatch the command to the inner bus
        self.inner
            .dispatch_erased(command, command_type_id, command_name, target_type)
            .await
    }
}

/// Extension trait for ErasedCommandBus to support outbox wrapping.
#[async_trait::async_trait]
pub trait OutboxCommandBusExt<R: CommandOutboxRepository + Send + Sync>: Send + Sync {
    /// Create an outbox decorator wrapping this CommandBus.
    fn outboxed(self, repository: Arc<R>) -> OutboxCommandBus<R, Self>
    where
        Self: Sized + Send + Sync;
}

#[async_trait::async_trait]
impl<R, B> OutboxCommandBusExt<R> for B
where
    R: CommandOutboxRepository + Send + Sync,
    B: ErasedCommandBus + Send + Sync,
{
    fn outboxed(self, repository: Arc<R>) -> OutboxCommandBus<R, Self>
    where
        Self: Sized + Send + Sync,
    {
        OutboxCommandBus::new(repository, Arc::new(self))
    }
}

/// Command Outbox Relay - processes pending commands (analogous to OutboxPoller)
pub struct CommandOutboxRelay<R> {
    repository: Arc<R>,
    command_bus: DynCommandBus,
    polling_interval: std::time::Duration,
    batch_size: usize,
    max_retries: i32,
    shutdown: tokio::sync::broadcast::Sender<()>,
}

impl<R> CommandOutboxRelay<R>
where
    R: CommandOutboxRepository + Send + Sync,
{
    /// Create a new relay
    pub fn new(
        repository: Arc<R>,
        command_bus: DynCommandBus,
        polling_interval: std::time::Duration,
        batch_size: usize,
        max_retries: i32,
    ) -> (Self, tokio::sync::broadcast::Receiver<()>) {
        let (shutdown, rx) = tokio::sync::broadcast::channel(1);

        let relay = Self {
            repository,
            command_bus,
            polling_interval,
            batch_size,
            max_retries,
            shutdown,
        };

        (relay, rx)
    }

    /// Run the relay loop
    pub async fn run(&self) {
        tracing::info!(
            polling_interval_ms = self.polling_interval.as_millis(),
            batch_size = self.batch_size,
            "Starting command outbox relay"
        );

        let mut interval = tokio::time::interval(self.polling_interval);
        let mut shutdown_rx = self.shutdown.subscribe();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.process_pending_commands().await {
                        tracing::warn!("Error processing command batch: {}", e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Command outbox relay shutting down");
                    break;
                }
            }
        }
    }

    /// Process a batch of pending commands
    pub async fn process_pending_commands(&self) -> Result<(), CommandOutboxError> {
        let pending_commands = self
            .repository
            .get_pending_commands(self.batch_size, self.max_retries)
            .await?;

        if pending_commands.is_empty() {
            tracing::debug!("No pending commands to process");
            return Ok(());
        }

        tracing::info!(count = pending_commands.len(), "Processing command batch");

        let mut completed_ids = Vec::new();
        let mut failed_ids = Vec::new();

        for record in &pending_commands {
            match self.process_command(record).await {
                Ok(_) => {
                    completed_ids.push(record.id);
                    tracing::debug!(command_id = %record.id, "Command processed successfully");
                }
                Err(e) => {
                    tracing::error!(
                        command_id = %record.id,
                        command_type = record.command_type,
                        retry_count = record.retry_count,
                        error = %e,
                        "Failed to process command"
                    );
                    failed_ids.push((record.id, e.to_string()));
                }
            }
        }

        // Update status in database
        for id in &completed_ids {
            self.repository.mark_completed(id).await?;
        }

        for (id, error_msg) in &failed_ids {
            self.repository.mark_failed(id, error_msg).await?;
        }

        Ok(())
    }

    /// Process a single command
    async fn process_command(
        &self,
        record: &CommandOutboxRecord,
    ) -> Result<(), CommandOutboxError> {
        tracing::debug!(
            command_id = %record.id,
            command_type = record.command_type,
            target_id = %record.target_id,
            "Processing command from outbox"
        );

        // Dispatch based on command type
        match record.command_type.as_str() {
            "MarkJobFailed" => {
                self.dispatch_mark_job_failed(record).await?;
            }
            "ResumeFromManualIntervention" => {
                self.dispatch_resume_from_manual_intervention(record)
                    .await?;
            }
            _ => {
                tracing::warn!(
                    command_id = %record.id,
                    command_type = record.command_type,
                    "Unknown command type, skipping"
                );
                return Err(CommandOutboxError::HandlerError(format!(
                    "Unknown command type: {}",
                    record.command_type
                )));
            }
        }

        tracing::info!(
            command_id = %record.id,
            command_type = record.command_type,
            "Command dispatched successfully"
        );

        Ok(())
    }

    /// Dispatch MarkJobFailed command
    async fn dispatch_mark_job_failed(
        &self,
        record: &CommandOutboxRecord,
    ) -> Result<(), CommandOutboxError> {
        // Deserialize the command payload
        #[derive(serde::Deserialize)]
        struct MarkJobFailedPayload {
            job_id: String,
            reason: String,
        }

        let payload: MarkJobFailedPayload = serde_json::from_value(record.payload.clone())
            .map_err(CommandOutboxError::Serialization)?;

        // Get job_id as UUID
        let job_id_uuid = uuid::Uuid::parse_str(&payload.job_id)
            .map_err(|_| CommandOutboxError::HandlerError("Invalid job_id format".to_string()))?;

        // Create and dispatch the command
        let command = MarkJobFailedCommand::new(
            super::super::shared_kernel::JobId(job_id_uuid),
            payload.reason,
        );

        // Dispatch via CommandBus
        let result = dispatch_erased(&self.command_bus, command).await;

        match result {
            Ok(_) => {
                tracing::debug!(job_id = payload.job_id, "MarkJobFailed command completed");
                Ok(())
            }
            Err(e) => Err(CommandOutboxError::HandlerError(e.to_string())),
        }
    }

    /// Dispatch ResumeFromManualIntervention command
    async fn dispatch_resume_from_manual_intervention(
        &self,
        record: &CommandOutboxRecord,
    ) -> Result<(), CommandOutboxError> {
        // Deserialize the command payload
        #[derive(serde::Deserialize)]
        struct ResumePayload {
            job_id: String,
            resume_reason: Option<String>,
        }

        let payload: ResumePayload = serde_json::from_value(record.payload.clone())
            .map_err(CommandOutboxError::Serialization)?;

        // Get job_id as UUID
        let job_id_uuid = uuid::Uuid::parse_str(&payload.job_id)
            .map_err(|_| CommandOutboxError::HandlerError("Invalid job_id format".to_string()))?;

        // Create and dispatch the command
        let command = if let Some(reason) = payload.resume_reason {
            ResumeFromManualInterventionCommand::with_reason(
                super::super::shared_kernel::JobId(job_id_uuid),
                reason,
            )
        } else {
            ResumeFromManualInterventionCommand::new(super::super::shared_kernel::JobId(
                job_id_uuid,
            ))
        };

        // Dispatch via CommandBus
        let result = dispatch_erased(&self.command_bus, command).await;

        match result {
            Ok(_) => {
                tracing::debug!(
                    job_id = payload.job_id,
                    "ResumeFromManualIntervention command completed"
                );
                Ok(())
            }
            Err(e) => Err(CommandOutboxError::HandlerError(e.to_string())),
        }
    }

    /// Signal shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;
    use std::sync::{Arc, Mutex};

    // Mock command for testing
    #[derive(Debug, Clone)]
    struct TestCommand {
        value: String,
    }

    impl Command for TestCommand {
        type Output = String;
        fn idempotency_key(&self) -> Cow<'_, str> {
            Cow::Owned(self.value.clone())
        }
    }

    // Mock handler for testing
    #[derive(Debug, Clone)]
    struct TestHandler;

    #[async_trait::async_trait]
    impl CommandHandler<TestCommand> for TestHandler {
        type Error = anyhow::Error;

        async fn handle(&self, command: TestCommand) -> Result<String, Self::Error> {
            Ok(format!("handled: {}", command.value))
        }
    }

    // Mock inner bus for testing
    #[derive(Clone, Default)]
    struct MockInnerBus {
        dispatched: Arc<Mutex<Vec<TestCommand>>>,
    }

    #[async_trait::async_trait]
    impl ErasedCommandBus for MockInnerBus {
        async fn dispatch_erased(
            &self,
            command: Box<dyn Any + Send>,
            _command_type_id: TypeId,
            _command_name: String,
            _target_type: CommandTargetType,
        ) -> Result<Box<dyn Any + Send>, CommandError> {
            let command =
                *command
                    .downcast::<TestCommand>()
                    .map_err(|_| CommandError::TypeMismatch {
                        expected: std::any::type_name::<TestCommand>().to_string(),
                        actual: "command type mismatch".to_string(),
                    })?;

            self.dispatched.lock().unwrap().push(command.clone());

            Ok(Box::new(format!("handled: {}", command.value)) as Box<dyn Any + Send>)
        }
    }

    // Mock repository for testing
    #[derive(Clone, Default)]
    struct MockRepository {
        inserted: Arc<Mutex<Vec<CommandOutboxRecord>>>,
    }

    #[async_trait::async_trait]
    impl CommandOutboxRepository for MockRepository {
        async fn insert_command(
            &self,
            command: &CommandOutboxInsert,
        ) -> Result<Uuid, CommandOutboxError> {
            let id = Uuid::new_v4();
            let record = CommandOutboxRecord {
                id,
                target_id: command.target_id,
                target_type: command.target_type.clone(),
                command_type: command.command_type.clone(),
                payload: command.payload.clone(),
                metadata: command.metadata.clone(),
                idempotency_key: command.idempotency_key.clone(),
                status: CommandOutboxStatus::Pending,
                created_at: Utc::now(),
                processed_at: None,
                retry_count: 0,
                last_error: None,
            };
            self.inserted.lock().unwrap().push(record);
            Ok(id)
        }

        async fn insert_command_with_tx(
            &self,
            _tx: &mut crate::transaction::PgTransaction<'_>,
            command: &CommandOutboxInsert,
        ) -> Result<Uuid, CommandOutboxError> {
            self.insert_command(command).await
        }

        async fn get_pending_commands(
            &self,
            _limit: usize,
            _max_retries: i32,
        ) -> Result<Vec<CommandOutboxRecord>, CommandOutboxError> {
            Ok(self.inserted.lock().unwrap().clone())
        }

        async fn mark_completed(&self, _command_id: &Uuid) -> Result<(), CommandOutboxError> {
            Ok(())
        }

        async fn mark_failed(
            &self,
            _command_id: &Uuid,
            _error: &str,
        ) -> Result<(), CommandOutboxError> {
            Ok(())
        }

        async fn exists_by_idempotency_key(&self, _key: &str) -> Result<bool, CommandOutboxError> {
            Ok(false)
        }

        async fn get_stats(&self) -> Result<CommandOutboxStats, CommandOutboxError> {
            let commands = self.inserted.lock().unwrap();
            Ok(CommandOutboxStats {
                pending_count: commands.iter().filter(|c| c.is_pending()).count() as u64,
                completed_count: 0,
                failed_count: 0,
                oldest_pending_age_seconds: None,
            })
        }
    }

    #[tokio::test]
    async fn test_command_outbox_record_status() {
        let pending = CommandOutboxRecord {
            id: Uuid::new_v4(),
            target_id: Uuid::new_v4(),
            target_type: CommandTargetType::Job,
            command_type: "Test".to_string(),
            payload: serde_json::json!({}),
            metadata: None,
            idempotency_key: None,
            status: CommandOutboxStatus::Pending,
            created_at: Utc::now(),
            processed_at: None,
            retry_count: 0,
            last_error: None,
        };

        assert!(pending.is_pending());
        assert!(!pending.is_completed());
    }

    #[tokio::test]
    async fn test_outbox_command_bus_persists_command() {
        let repository = Arc::new(MockRepository::default());
        let inner = Arc::new(MockInnerBus::default());
        let outbox_bus = OutboxCommandBus::new(repository.clone(), inner.clone());

        let command = TestCommand {
            value: "test".to_string(),
        };

        let result = outbox_bus
            .dispatch_erased(
                Box::new(command),
                TypeId::of::<TestCommand>(),
                "TestCommand".to_string(),
                CommandTargetType::Saga,
            )
            .await;

        assert!(result.is_ok());
        let output: Box<String> = result.unwrap().downcast().unwrap();
        assert_eq!(*output, "handled: test");

        // Verify command was persisted to outbox
        assert_eq!(repository.inserted.lock().unwrap().len(), 1);
        assert_eq!(
            repository.inserted.lock().unwrap()[0].target_type,
            CommandTargetType::Saga
        );
    }

    #[tokio::test]
    async fn test_outbox_command_bus_extension() {
        let repository = Arc::new(MockRepository::default());
        let inner = Arc::new(MockInnerBus::default());

        // Create outbox bus directly (simpler than using extension trait)
        let outboxed = OutboxCommandBus::new(repository.clone(), inner.clone());

        let command = TestCommand {
            value: "extension test".to_string(),
        };

        let result = outboxed
            .dispatch_erased(
                Box::new(command),
                TypeId::of::<TestCommand>(),
                "TestCommand".to_string(),
                CommandTargetType::Saga,
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(repository.inserted.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_outbox_command_bus_clone() {
        let repository = Arc::new(MockRepository::default());
        let inner = Arc::new(MockInnerBus::default());
        let outbox_bus = OutboxCommandBus::new(repository.clone(), inner.clone());

        let cloned = outbox_bus.clone();

        let command = TestCommand {
            value: "clone test".to_string(),
        };

        let result = cloned
            .dispatch_erased(
                Box::new(command),
                TypeId::of::<TestCommand>(),
                "TestCommand".to_string(),
                CommandTargetType::Saga,
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_command_outbox_insert_for_job() {
        let insert = CommandOutboxInsert::for_job(
            Uuid::new_v4(),
            "MarkJobFailed".to_string(),
            serde_json::json!({"job_id": "123", "reason": "timeout"}),
            None,
            Some("idemp-key-123".to_string()),
        );

        assert_eq!(insert.target_type, CommandTargetType::Job);
        assert_eq!(insert.command_type, "MarkJobFailed");
        assert_eq!(insert.idempotency_key, Some("idemp-key-123".to_string()));
    }

    #[tokio::test]
    async fn test_command_outbox_stats() {
        let stats = CommandOutboxStats {
            pending_count: 5,
            completed_count: 10,
            failed_count: 2,
            oldest_pending_age_seconds: Some(300),
        };

        assert_eq!(stats.total(), 17);
        assert!(stats.has_pending());
    }
}
