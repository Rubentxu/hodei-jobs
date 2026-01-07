// Command Outbox Module
//
// Implements the Transactional Outbox Pattern for commands.
// Commands are persisted to the hodei_commands table and processed by a background relay.

use super::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

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

/// A command record for the outbox
#[derive(Debug, Clone)]
pub struct CommandOutboxRecord {
    pub id: Uuid,
    pub command_type: String,
    pub target_id: String,
    pub target_type: String,
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
}

/// Trait for command outbox repository
#[async_trait]
pub trait CommandOutboxRepository: Send + Sync {
    /// Insert a command into the outbox
    async fn insert_command(
        &self,
        command_type: &str,
        target_id: &str,
        target_type: &str,
        payload: serde_json::Value,
        idempotency_key: Option<&str>,
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

/// Command Outbox Relay - processes pending commands
pub struct CommandOutboxRelay<R, B> {
    repository: Arc<R>,
    command_bus: Arc<B>,
    polling_interval: std::time::Duration,
    batch_size: usize,
    max_retries: i32,
    shutdown: tokio::sync::broadcast::Sender<()>,
}

impl<R, B> CommandOutboxRelay<R, B>
where
    R: CommandOutboxRepository + Send + Sync,
    B: CommandBus + Send + Sync,
{
    /// Create a new relay
    pub fn new(
        repository: Arc<R>,
        command_bus: Arc<B>,
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
        // This is a simplified implementation
        // In practice, you'd deserialize the command and dispatch it
        tracing::debug!(command_type = record.command_type, "Processing command");
        Ok(())
    }

    /// Signal shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Mock command outbox repository for testing
    struct MockCommandOutboxRepository {
        commands: Arc<Mutex<Vec<CommandOutboxRecord>>>,
    }

    impl MockCommandOutboxRepository {
        fn new() -> Self {
            Self {
                commands: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl CommandOutboxRepository for MockCommandOutboxRepository {
        async fn insert_command(
            &self,
            command_type: &str,
            target_id: &str,
            target_type: &str,
            payload: serde_json::Value,
            _idempotency_key: Option<&str>,
        ) -> Result<Uuid, CommandOutboxError> {
            let id = Uuid::new_v4();
            let mut commands = self.commands.lock().unwrap();
            commands.push(CommandOutboxRecord {
                id,
                command_type: command_type.to_string(),
                target_id: target_id.to_string(),
                target_type: target_type.to_string(),
                payload,
                metadata: None,
                idempotency_key: None,
                status: CommandOutboxStatus::Pending,
                created_at: Utc::now(),
                processed_at: None,
                retry_count: 0,
                last_error: None,
            });
            Ok(id)
        }

        async fn get_pending_commands(
            &self,
            limit: usize,
            _max_retries: i32,
        ) -> Result<Vec<CommandOutboxRecord>, CommandOutboxError> {
            let commands = self.commands.lock().unwrap();
            Ok(commands
                .iter()
                .filter(|c| c.is_pending())
                .take(limit)
                .cloned()
                .collect())
        }

        async fn mark_completed(&self, command_id: &Uuid) -> Result<(), CommandOutboxError> {
            let mut commands = self.commands.lock().unwrap();
            if let Some(cmd) = commands.iter_mut().find(|c| &c.id == command_id) {
                cmd.status = CommandOutboxStatus::Completed;
                cmd.processed_at = Some(Utc::now());
            }
            Ok(())
        }

        async fn mark_failed(
            &self,
            command_id: &Uuid,
            error: &str,
        ) -> Result<(), CommandOutboxError> {
            let mut commands = self.commands.lock().unwrap();
            if let Some(cmd) = commands.iter_mut().find(|c| &c.id == command_id) {
                cmd.status = CommandOutboxStatus::Failed;
                cmd.retry_count += 1;
                cmd.last_error = Some(error.to_string());
            }
            Ok(())
        }

        async fn exists_by_idempotency_key(&self, _key: &str) -> Result<bool, CommandOutboxError> {
            Ok(false)
        }

        async fn get_stats(&self) -> Result<CommandOutboxStats, CommandOutboxError> {
            let commands = self.commands.lock().unwrap();
            let pending_count = commands.iter().filter(|c| c.is_pending()).count() as u64;
            let completed_count = commands.iter().filter(|c| c.is_completed()).count() as u64;
            let failed_count = commands
                .iter()
                .filter(|c| matches!(c.status, CommandOutboxStatus::Failed))
                .count() as u64;

            Ok(CommandOutboxStats {
                pending_count,
                completed_count,
                failed_count,
                oldest_pending_age_seconds: None,
            })
        }
    }

    #[tokio::test]
    async fn test_command_outbox_record_status() {
        let pending = CommandOutboxRecord {
            id: Uuid::new_v4(),
            command_type: "Test".to_string(),
            target_id: "target-1".to_string(),
            target_type: "Job".to_string(),
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
    async fn test_command_outbox_repository() {
        let repo = Arc::new(MockCommandOutboxRepository::new());

        let id = repo
            .insert_command("TestCommand", "job-123", "Job", serde_json::json!({}), None)
            .await
            .unwrap();

        assert!(!id.is_nil());

        let pending = repo.get_pending_commands(10, 3).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].target_id, "job-123");
    }

    #[tokio::test]
    async fn test_mark_completed() {
        let repo = Arc::new(MockCommandOutboxRepository::new());

        let id = repo
            .insert_command("TestCommand", "job-123", "Job", serde_json::json!({}), None)
            .await
            .unwrap();

        assert!(repo.get_pending_commands(10, 3).await.unwrap().len() == 1);

        repo.mark_completed(&id).await.unwrap();

        assert!(repo.get_pending_commands(10, 3).await.unwrap().is_empty());

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.completed_count, 1);
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let repo = Arc::new(MockCommandOutboxRepository::new());

        let id = repo
            .insert_command("TestCommand", "job-123", "Job", serde_json::json!({}), None)
            .await
            .unwrap();

        repo.mark_failed(&id, "Handler error").await.unwrap();

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.failed_count, 1);
    }
}
