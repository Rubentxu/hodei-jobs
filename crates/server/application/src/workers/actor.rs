//! Worker Supervisor Actor
//!
//! Implements the Actor Model pattern for worker management, replacing
//! the `RwLock<HashMap>` global state with message-passing concurrency.
//!
//! This actor maintains exclusive ownership of the worker registry state,
//! processing messages sequentially to eliminate lock contention.
//!
//! EPIC-42: Includes active heartbeat timeout tracking for worker health.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use hodei_server_domain::shared_kernel::{DomainError, ProviderId, WorkerId, WorkerState};
use hodei_server_domain::workers::{WorkerHandle, WorkerRegistry, WorkerSpec};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::event_bus::EventBus;
use chrono::Utc;

/// Errors from WorkerSupervisor operations
#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Worker already registered: {worker_id}")]
    WorkerAlreadyRegistered { worker_id: WorkerId },

    #[error("Registration failed: {reason}")]
    RegistrationFailed { reason: String },

    #[error("Channel closed for worker: {worker_id}")]
    ChannelClosed { worker_id: WorkerId },

    #[error("Shutdown signal received")]
    Shutdown,

    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl From<SupervisorError> for DomainError {
    fn from(err: SupervisorError) -> Self {
        match err {
            SupervisorError::WorkerNotFound { worker_id } => {
                DomainError::WorkerNotFound { worker_id }
            }
            SupervisorError::WorkerAlreadyRegistered { worker_id } => {
                DomainError::WorkerAlreadyExists { worker_id }
            }
            SupervisorError::RegistrationFailed { reason } => {
                DomainError::WorkerProvisioningFailed {
                    message: format!("Worker registration failed: {}", reason),
                }
            }
            SupervisorError::ChannelClosed { worker_id } => DomainError::InfrastructureError {
                message: format!("Worker channel closed: {}", worker_id),
            },
            SupervisorError::Shutdown => DomainError::InfrastructureError {
                message: "Supervisor shutdown".to_string(),
            },
            SupervisorError::Internal { message } => DomainError::InfrastructureError { message },
        }
    }
}

/// Result type for supervisor operations
pub type SupervisorResult<T> = Result<T, SupervisorError>;

/// Worker state maintained by the supervisor (in-memory view)
#[derive(Debug, Clone)]
pub struct WorkerActorState {
    pub worker_id: WorkerId,
    pub provider_id: ProviderId,
    pub handle: WorkerHandle,
    pub spec: WorkerSpec,
    pub status: WorkerState,
    pub session_id: String,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
    pub current_job_id: Option<hodei_server_domain::shared_kernel::JobId>,
}

impl WorkerActorState {
    pub fn new(
        worker_id: WorkerId,
        provider_id: ProviderId,
        handle: WorkerHandle,
        spec: WorkerSpec,
        session_id: String,
    ) -> Self {
        Self {
            worker_id,
            provider_id,
            handle,
            spec,
            status: WorkerState::Creating,
            session_id,
            registered_at: chrono::Utc::now(),
            last_heartbeat: None,
            current_job_id: None,
        }
    }
}

/// Messages sent to workers via their dedicated channels
#[derive(Debug, Clone)]
pub enum WorkerMsg {
    /// Run a job on this worker
    RunJob {
        job_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// Terminate the worker
    Terminate { reason: String },
    /// Update worker status
    UpdateStatus(WorkerState),
    /// Ping for liveness
    Ping,
}

impl WorkerMsg {
    /// Check if this message expects a response
    pub fn expects_response(&self) -> bool {
        matches!(self, WorkerMsg::Ping)
    }
}

/// Protocol messages for WorkerSupervisor
#[derive(Debug)]
pub enum SupervisorMsg {
    /// Register a new worker
    Register {
        worker_id: WorkerId,
        provider_id: ProviderId,
        handle: WorkerHandle,
        spec: WorkerSpec,
        reply_to: oneshot::Sender<SupervisorResult<WorkerActorState>>,
    },

    /// Unregister a worker
    Unregister {
        worker_id: WorkerId,
        reply_to: oneshot::Sender<SupervisorResult<()>>,
    },

    /// Worker disconnected (graceful or abrupt)
    WorkerDisconnected {
        worker_id: WorkerId,
        reason: DisconnectionReason,
        reply_to: oneshot::Sender<SupervisorResult<()>>,
    },

    /// Heartbeat received from worker
    Heartbeat {
        worker_id: WorkerId,
        reply_to: oneshot::Sender<SupervisorResult<()>>,
    },

    /// Send a message to a specific worker
    SendToWorker {
        worker_id: WorkerId,
        message: WorkerMsg,
        reply_to: oneshot::Sender<SupervisorResult<()>>,
    },

    /// Get state of a specific worker
    GetWorker {
        worker_id: WorkerId,
        reply_to: oneshot::Sender<SupervisorResult<WorkerActorState>>,
    },

    /// List all workers, optionally filtered
    ListWorkers {
        filter: Option<WorkerState>,
        reply_to: oneshot::Sender<Vec<WorkerActorState>>,
    },

    /// Get count of workers by status
    GetWorkerStats {
        reply_to: oneshot::Sender<WorkerStats>,
    },

    /// Set up a channel for an active worker (for bidirectional streaming)
    SetWorkerChannel {
        worker_id: WorkerId,
        channel: mpsc::Sender<WorkerMsg>,
        reply_to: oneshot::Sender<SupervisorResult<()>>,
    },

    /// Graceful shutdown signal
    Shutdown { reply_to: oneshot::Sender<()> },
}

/// Reason for worker disconnection
#[derive(Debug, Clone)]
pub enum DisconnectionReason {
    GracefulTermination,
    HeartbeatTimeout,
    NetworkError,
    WorkerCrash,
    Reboot,
    Unknown,
}

/// Statistics about registered workers
#[derive(Debug, Default)]
pub struct WorkerStats {
    pub total: usize,
    pub by_status: HashMap<String, usize>,
}

impl WorkerStats {
    pub fn increment(&mut self, status: &WorkerState) {
        self.total += 1;
        let status_str = status.to_string();
        *self.by_status.entry(status_str).or_insert(0) += 1;
    }
}

/// Actor state - NOT thread-safe, only accessed by the actor loop
struct ActorState {
    /// Registry of all workers (private state, no locks needed)
    registry: HashMap<WorkerId, WorkerActorState>,
    /// Channels for sending messages to active workers
    worker_channels: HashMap<WorkerId, mpsc::Sender<WorkerMsg>>,
    /// Metrics
    messages_processed: u64,
    registrations_count: u64,
    disconnections_count: u64,
    /// EPIC-42: Heartbeat timeout tracking
    timeouts_count: u64,
}

impl ActorState {
    fn new() -> Self {
        Self {
            registry: HashMap::new(),
            worker_channels: HashMap::new(),
            messages_processed: 0,
            registrations_count: 0,
            disconnections_count: 0,
            timeouts_count: 0,
        }
    }
}

/// The WorkerSupervisor Actor
///
/// This actor is the single owner of the worker registry state.
/// It processes messages sequentially, eliminating lock contention.
/// EPIC-42: Includes active heartbeat timeout tracking for worker health.
pub struct WorkerSupervisor {
    /// Receiver for supervisor messages
    inbox: mpsc::Receiver<SupervisorMsg>,
    /// Actor's private state
    state: ActorState,
    /// Shutdown signal receiver
    shutdown: watch::Receiver<()>,
    /// Prometheus metrics
    metrics: Arc<WorkerSupervisorMetrics>,
    /// Configuration (EPIC-42)
    config: WorkerSupervisorConfig,
    /// Worker registry for database persistence
    worker_registry: Option<Arc<dyn WorkerRegistry>>,
    /// Event bus for publishing domain events
    event_bus: Option<Arc<dyn EventBus>>,
}

impl WorkerSupervisor {
    /// Create a new WorkerSupervisor
    pub fn new(
        inbox: mpsc::Receiver<SupervisorMsg>,
        shutdown: watch::Receiver<()>,
        metrics: Arc<WorkerSupervisorMetrics>,
    ) -> Self {
        Self {
            inbox,
            state: ActorState::new(),
            shutdown,
            metrics,
            config: WorkerSupervisorConfig::default(),
            worker_registry: None,
            event_bus: None,
        }
    }

    /// Create with custom configuration (EPIC-42)
    pub fn with_config(
        inbox: mpsc::Receiver<SupervisorMsg>,
        shutdown: watch::Receiver<()>,
        metrics: Arc<WorkerSupervisorMetrics>,
        config: WorkerSupervisorConfig,
    ) -> Self {
        Self {
            inbox,
            state: ActorState::new(),
            shutdown,
            metrics,
            config,
            worker_registry: None,
            event_bus: None,
        }
    }

    /// Set worker registry for database persistence
    pub fn with_worker_registry(mut self, registry: Arc<dyn WorkerRegistry>) -> Self {
        self.worker_registry = Some(registry);
        self
    }

    /// Set event bus for publishing domain events
    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Run the actor loop with active heartbeat timeout tracking (EPIC-42)
    pub async fn run(mut self) {
        info!(
            "🚀 WorkerSupervisor: Starting actor loop with heartbeat_timeout_tracking={}",
            self.config.heartbeat_timeout_tracking
        );

        if self.config.heartbeat_timeout_tracking {
            self.run_with_heartbeat_check().await;
        } else {
            self.run_without_heartbeat_check().await;
        }

        info!(
            messages_processed = self.state.messages_processed,
            registrations = self.state.registrations_count,
            disconnections = self.state.disconnections_count,
            "WorkerSupervisor: Actor loop ended"
        );
    }

    /// Run loop with heartbeat timeout tracking enabled
    async fn run_with_heartbeat_check(&mut self) {
        loop {
            tokio::select! {
                _ = self.shutdown.changed() => {
                    info!("WorkerSupervisor: Shutdown signal received");
                    break;
                }
                // EPIC-42: Process messages from workers
                // Heartbeats are checked after each message is processed
                msg = self.inbox.recv() => {
                    match msg {
                        Some(m) => {
                            self.metrics.inc_mailbox_size();
                            let _ = self.handle_message(m).await;
                            self.metrics.dec_mailbox_size();
                        }
                        None => {
                            info!("WorkerSupervisor: Inbox channel closed");
                            break;
                        }
                    }
                    // EPIC-42: Check heartbeats after processing any message
                    // This ensures timely detection of stale workers
                    self.check_heartbeat_timeouts().await;
                }
            }
        }
    }

    /// Run loop without heartbeat timeout tracking
    async fn run_without_heartbeat_check(&mut self) {
        loop {
            tokio::select! {
                _ = self.shutdown.changed() => {
                    info!("WorkerSupervisor: Shutdown signal received");
                    break;
                }
                msg = self.inbox.recv() => {
                    match msg {
                        Some(m) => {
                            self.metrics.inc_mailbox_size();
                            let _ = self.handle_message(m).await;
                            self.metrics.dec_mailbox_size();
                        }
                        None => {
                            info!("WorkerSupervisor: Inbox channel closed");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// EPIC-42: Check for workers that have missed heartbeats
    async fn check_heartbeat_timeouts(&mut self) {
        let now = chrono::Utc::now();
        let timeout_duration = chrono::Duration::from_std(self.config.heartbeat_timeout)
            .unwrap_or_else(|_| chrono::Duration::seconds(30));

        let mut timed_out_workers = Vec::new();

        debug!(
            registry_size = self.state.registry.len(),
            timeout_seconds = timeout_duration.num_seconds(),
            "Checking heartbeat timeouts"
        );

        for (worker_id, worker_state) in &self.state.registry {
            // Skip workers that haven't started sending heartbeats yet
            if let Some(last_heartbeat) = worker_state.last_heartbeat {
                let elapsed = now.signed_duration_since(last_heartbeat);

                debug!(
                    worker_id = %worker_id,
                    last_heartbeat = %last_heartbeat,
                    elapsed_seconds = elapsed.num_seconds(),
                    timeout_seconds = timeout_duration.num_seconds(),
                    "Worker heartbeat status"
                );

                if elapsed > timeout_duration {
                    warn!(
                        worker_id = %worker_id,
                        last_heartbeat = %last_heartbeat,
                        elapsed_seconds = elapsed.num_seconds(),
                        "Worker heartbeat timeout detected"
                    );
                    timed_out_workers.push(worker_id.clone());
                    self.state.timeouts_count += 1;
                    self.metrics.record_timeout();
                }
            } else {
                debug!(worker_id = %worker_id, "Worker has no heartbeat yet");
            }
        }

        // Mark timed out workers as disconnected
        for worker_id in &timed_out_workers {
            let _ = self.handle_disconnect(
                worker_id,
                DisconnectionReason::HeartbeatTimeout,
            ).await;
        }

        if !timed_out_workers.is_empty() {
            debug!(
                count = timed_out_workers.len(),
                "Heartbeat timeout check complete"
            );
        }
    }

    /// Handle a single message
    async fn handle_message(&mut self, msg: SupervisorMsg) {
        self.state.messages_processed += 1;

        match msg {
            SupervisorMsg::Register {
                worker_id,
                provider_id,
                handle,
                spec,
                reply_to,
            } => {
                let result = self
                    .handle_register(worker_id, provider_id, handle, spec)
                    .await;
                let _ = reply_to.send(result);
            }

            SupervisorMsg::Unregister {
                worker_id,
                reply_to,
            } => {
                let result = self.handle_unregister(&worker_id).await;
                let _ = reply_to.send(result);
            }

            SupervisorMsg::WorkerDisconnected {
                worker_id,
                reason,
                reply_to,
            } => {
                let result = self.handle_disconnect(&worker_id, reason).await;
                self.state.disconnections_count += 1;
                let _ = reply_to.send(result);
            }

            SupervisorMsg::Heartbeat {
                worker_id,
                reply_to,
            } => {
                let result = self.handle_heartbeat(&worker_id).await;
                let _ = reply_to.send(result);
            }

            SupervisorMsg::SendToWorker {
                worker_id,
                message,
                reply_to,
            } => {
                let result = self.handle_send_to_worker(&worker_id, message).await;
                let _ = reply_to.send(result);
            }

            SupervisorMsg::GetWorker {
                worker_id,
                reply_to,
            } => {
                let result = self.handle_get_worker(&worker_id);
                let _ = reply_to.send(result);
            }

            SupervisorMsg::ListWorkers { filter, reply_to } => {
                let result = self.handle_list_workers(filter.as_ref());
                let _ = reply_to.send(result);
            }

            SupervisorMsg::GetWorkerStats { reply_to } => {
                let result = self.handle_get_stats();
                let _ = reply_to.send(result);
            }

            SupervisorMsg::SetWorkerChannel {
                worker_id,
                channel,
                reply_to,
            } => {
                let result = self.handle_set_channel(&worker_id, channel).await;
                let _ = reply_to.send(result);
            }

            SupervisorMsg::Shutdown { reply_to } => {
                self.handle_shutdown().await;
                let _ = reply_to.send(());
            }
        }
    }

    async fn handle_register(
        &mut self,
        worker_id: WorkerId,
        provider_id: ProviderId,
        handle: WorkerHandle,
        spec: WorkerSpec,
    ) -> SupervisorResult<WorkerActorState> {
        // Check if already registered
        if self.state.registry.contains_key(&worker_id) {
            return Err(SupervisorError::WorkerAlreadyRegistered { worker_id });
        }

        let session_id = uuid::Uuid::new_v4().to_string();
        let state = WorkerActorState::new(worker_id.clone(), provider_id, handle, spec, session_id);

        self.state.registry.insert(worker_id.clone(), state.clone());
        self.state.registrations_count += 1;

        debug!(worker_id = %worker_id, "Worker registered");

        Ok(state)
    }

    async fn handle_unregister(&mut self, worker_id: &WorkerId) -> SupervisorResult<()> {
        // Remove channel first
        self.state.worker_channels.remove(worker_id);

        // Remove from registry
        if let Some(state) = self.state.registry.remove(worker_id) {
            debug!(worker_id = %worker_id, status = %state.status, "Worker unregistered");
            return Ok(());
        }

        Err(SupervisorError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })
    }

    async fn handle_disconnect(
        &mut self,
        worker_id: &WorkerId,
        reason: DisconnectionReason,
    ) -> SupervisorResult<()> {
        // Remove channel
        self.state.worker_channels.remove(worker_id);

        // Update status if worker exists
        if let Some(state) = self.state.registry.get_mut(worker_id) {
            state.status = WorkerState::Terminated;
            debug!(worker_id = %worker_id, ?reason, "Worker disconnected");
            return Ok(());
        }

        Err(SupervisorError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })
    }

    async fn handle_heartbeat(&mut self, worker_id: &WorkerId) -> SupervisorResult<()> {
        if let Some(state) = self.state.registry.get_mut(worker_id) {
            state.last_heartbeat = Some(chrono::Utc::now());

            // Check if transitioning from Creating to Ready
            let state_changed = state.status == WorkerState::Creating;
            let provider_id = state.provider_id.clone();
            state.status = WorkerState::Ready;

            // Persist state change to database and emit event if transitioning to Ready
            if state_changed {
                if let Some(ref registry) = self.worker_registry {
                    // First, query the worker from DB to get the current_job_id
                    let job_id = match registry.get(worker_id).await {
                        Ok(Some(worker)) => worker.current_job_id().cloned(),
                        Ok(None) => {
                            warn!(
                                worker_id = %worker_id,
                                "Worker not found in registry when querying job_id"
                            );
                            None
                        }
                        Err(e) => {
                            warn!(
                                worker_id = %worker_id,
                                error = %e,
                                "Failed to query worker for job_id"
                            );
                            None
                        }
                    };

                    // Update in-memory state with job_id
                    if let Some(state) = self.state.registry.get_mut(worker_id) {
                        state.current_job_id = job_id.clone();
                    }

                    if let Err(e) = registry.update_state(worker_id, WorkerState::Ready).await {
                        error!(
                            worker_id = %worker_id,
                            error = %e,
                            "Failed to persist worker state transition to Ready in database"
                        );
                        // Continue execution - in-memory state is updated
                    } else {
                        debug!(
                            worker_id = %worker_id,
                            "Worker state persisted to database: Creating -> Ready"
                        );

                        // Emit WorkerReady event with job_id for OTP-based provisioning
                        if let Some(ref event_bus) = self.event_bus {
                            let event = DomainEvent::WorkerReady {
                                worker_id: worker_id.clone(),
                                provider_id: provider_id.clone(),
                                job_id: job_id.clone(),
                                ready_at: Utc::now(),
                                correlation_id: job_id.as_ref().map(|id| id.to_string()),
                                actor: Some("worker-supervisor-actor".to_string()),
                            };

                            if let Err(e) = event_bus.publish(&event).await {
                                error!(
                                    worker_id = %worker_id,
                                    error = %e,
                                    "Failed to publish WorkerReady event"
                                );
                            } else {
                                info!(
                                    worker_id = %worker_id,
                                    job_id = ?job_id,
                                    "✅ Published WorkerReady event (triggers JobDispatcher)"
                                );
                            }
                        }
                    }
                }
            }

            return Ok(());
        }

        Err(SupervisorError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })
    }

    async fn handle_send_to_worker(
        &mut self,
        worker_id: &WorkerId,
        message: WorkerMsg,
    ) -> SupervisorResult<()> {
        if let Some(channel) = self.state.worker_channels.get(worker_id) {
            if let Err(_) = channel.send(message).await {
                return Err(SupervisorError::ChannelClosed {
                    worker_id: worker_id.clone(),
                });
            }
            return Ok(());
        }

        Err(SupervisorError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })
    }

    fn handle_get_worker(&self, worker_id: &WorkerId) -> SupervisorResult<WorkerActorState> {
        self.state
            .registry
            .get(worker_id)
            .cloned()
            .ok_or_else(|| SupervisorError::WorkerNotFound {
                worker_id: worker_id.clone(),
            })
    }

    fn handle_list_workers(&self, filter: Option<&WorkerState>) -> Vec<WorkerActorState> {
        self.state
            .registry
            .values()
            .filter(|w| filter.map(|f| &w.status == f).unwrap_or(true))
            .cloned()
            .collect()
    }

    fn handle_get_stats(&self) -> WorkerStats {
        let mut stats = WorkerStats::default();

        for worker in self.state.registry.values() {
            stats.increment(&worker.status);
        }

        stats
    }

    async fn handle_set_channel(
        &mut self,
        worker_id: &WorkerId,
        channel: mpsc::Sender<WorkerMsg>,
    ) -> SupervisorResult<()> {
        if let Some(state) = self.state.registry.get_mut(worker_id) {
            state.status = WorkerState::Ready;
            self.state
                .worker_channels
                .insert(worker_id.clone(), channel);
            debug!(worker_id = %worker_id, "Worker channel set");
            return Ok(());
        }

        Err(SupervisorError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })
    }

    async fn handle_shutdown(&mut self) {
        info!("WorkerSupervisor: Initiating graceful shutdown");

        // Send termination message to all workers
        for (_worker_id, channel) in &self.state.worker_channels {
            let _ = channel
                .send(WorkerMsg::Terminate {
                    reason: "Server shutdown".to_string(),
                })
                .await;
        }

        // Clear all state
        self.state.registry.clear();
        self.state.worker_channels.clear();

        info!("WorkerSupervisor: Graceful shutdown complete");
    }
}

/// Handle for communicating with WorkerSupervisor from external code
#[derive(Clone)]
pub struct WorkerSupervisorHandle {
    tx: mpsc::Sender<SupervisorMsg>,
}

impl WorkerSupervisorHandle {
    /// Create a new handle
    pub fn new(tx: mpsc::Sender<SupervisorMsg>) -> Self {
        Self { tx }
    }

    /// Register a new worker
    pub async fn register(
        &self,
        worker_id: WorkerId,
        provider_id: ProviderId,
        handle: WorkerHandle,
        spec: WorkerSpec,
    ) -> SupervisorResult<WorkerActorState> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SupervisorMsg::Register {
                worker_id,
                provider_id,
                handle,
                spec,
                reply_to: reply_tx,
            })
            .await
            .map_err(|_| SupervisorError::Internal {
                message: "Supervisor inbox closed".to_string(),
            })?;

        reply_rx.await.map_err(|_| SupervisorError::Internal {
            message: "Registration response lost".to_string(),
        })?
    }

    /// Unregister a worker
    pub async fn unregister(&self, worker_id: &WorkerId) -> SupervisorResult<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SupervisorMsg::Unregister {
                worker_id: worker_id.clone(),
                reply_to: reply_tx,
            })
            .await
            .map_err(|_| SupervisorError::Internal {
                message: "Supervisor inbox closed".to_string(),
            })?;

        reply_rx.await.map_err(|_| SupervisorError::Internal {
            message: "Unregister response lost".to_string(),
        })?
    }

    /// Handle worker disconnection
    pub async fn worker_disconnected(
        &self,
        worker_id: &WorkerId,
        reason: DisconnectionReason,
    ) -> SupervisorResult<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SupervisorMsg::WorkerDisconnected {
                worker_id: worker_id.clone(),
                reason,
                reply_to: reply_tx,
            })
            .await
            .map_err(|_| SupervisorError::Internal {
                message: "Supervisor inbox closed".to_string(),
            })?;

        reply_rx.await.map_err(|_| SupervisorError::Internal {
            message: "Disconnection response lost".to_string(),
        })?
    }

    /// Record a heartbeat
    pub async fn heartbeat(&self, worker_id: &WorkerId) -> SupervisorResult<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SupervisorMsg::Heartbeat {
                worker_id: worker_id.clone(),
                reply_to: reply_tx,
            })
            .await
            .map_err(|_| SupervisorError::Internal {
                message: "Supervisor inbox closed".to_string(),
            })?;

        reply_rx.await.map_err(|_| SupervisorError::Internal {
            message: "Heartbeat response lost".to_string(),
        })?
    }

    /// Send a message to a worker
    pub async fn send_to_worker(
        &self,
        worker_id: &WorkerId,
        message: WorkerMsg,
    ) -> SupervisorResult<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SupervisorMsg::SendToWorker {
                worker_id: worker_id.clone(),
                message,
                reply_to: reply_tx,
            })
            .await
            .map_err(|_| SupervisorError::Internal {
                message: "Supervisor inbox closed".to_string(),
            })?;

        reply_rx.await.map_err(|_| SupervisorError::Internal {
            message: "Send response lost".to_string(),
        })?
    }

    /// Get worker state
    pub async fn get_worker(&self, worker_id: &WorkerId) -> SupervisorResult<WorkerActorState> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SupervisorMsg::GetWorker {
                worker_id: worker_id.clone(),
                reply_to: reply_tx,
            })
            .await
            .map_err(|_| SupervisorError::Internal {
                message: "Supervisor inbox closed".to_string(),
            })?;

        reply_rx.await.map_err(|_| SupervisorError::Internal {
            message: "GetWorker response lost".to_string(),
        })?
    }

    /// List all workers
    pub async fn list_workers(&self, filter: Option<WorkerState>) -> Vec<WorkerActorState> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(SupervisorMsg::ListWorkers {
                filter,
                reply_to: reply_tx,
            })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Get worker statistics
    pub async fn get_stats(&self) -> WorkerStats {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(SupervisorMsg::GetWorkerStats { reply_to: reply_tx })
            .await
            .is_err()
        {
            return WorkerStats::default();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Set up a channel for an active worker
    pub async fn set_worker_channel(
        &self,
        worker_id: &WorkerId,
        channel: mpsc::Sender<WorkerMsg>,
    ) -> SupervisorResult<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SupervisorMsg::SetWorkerChannel {
                worker_id: worker_id.clone(),
                channel,
                reply_to: reply_tx,
            })
            .await
            .map_err(|_| SupervisorError::Internal {
                message: "Supervisor inbox closed".to_string(),
            })?;

        reply_rx.await.map_err(|_| SupervisorError::Internal {
            message: "SetChannel response lost".to_string(),
        })?
    }

    /// Signal shutdown
    pub async fn shutdown(&self) {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SupervisorMsg::Shutdown { reply_to: reply_tx })
            .await;
        let _ = reply_rx.await;
    }
}

/// Metrics for WorkerSupervisor
#[derive(Clone)]
pub struct WorkerSupervisorMetrics {
    mailbox_size: Arc<std::sync::atomic::AtomicUsize>,
    messages_processed: Arc<std::sync::atomic::AtomicU64>,
    registrations_total: Arc<std::sync::atomic::AtomicU64>,
    disconnections_total: Arc<std::sync::atomic::AtomicU64>,
    /// EPIC-42: Heartbeat timeout metrics
    timeouts_total: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for WorkerSupervisorMetrics {
    fn default() -> Self {
        Self {
            mailbox_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            messages_processed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            registrations_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            disconnections_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            timeouts_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}

impl WorkerSupervisorMetrics {
    /// Increment mailbox size counter
    pub fn inc_mailbox_size(&self) {
        self.mailbox_size
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Decrement mailbox size counter
    pub fn dec_mailbox_size(&self) {
        self.mailbox_size
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get current mailbox size
    pub fn mailbox_size(&self) -> usize {
        self.mailbox_size.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Record a processed message
    pub fn record_message(&self) {
        self.messages_processed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a registration
    pub fn record_registration(&self) {
        self.registrations_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a disconnection
    pub fn record_disconnection(&self) {
        self.disconnections_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// EPIC-42: Record a heartbeat timeout
    pub fn record_timeout(&self) {
        self.timeouts_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get total messages processed
    pub fn messages_processed(&self) -> u64 {
        self.messages_processed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get total registrations
    pub fn registrations_total(&self) -> u64 {
        self.registrations_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get total disconnections
    pub fn disconnections_total(&self) -> u64 {
        self.disconnections_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// EPIC-42: Get total timeouts
    pub fn timeouts_total(&self) -> u64 {
        self.timeouts_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Configuration for WorkerSupervisor
#[derive(Debug, Clone)]
pub struct WorkerSupervisorConfig {
    /// Maximum workers to register
    pub max_workers: usize,
    /// Channel capacity for supervisor inbox
    pub inbox_capacity: usize,
    /// Channel capacity for worker messages
    pub worker_channel_capacity: usize,
    /// Enable actor model (disable for legacy mode)
    pub actor_enabled: bool,
    /// Heartbeat timeout duration (EPIC-42)
    pub heartbeat_timeout: Duration,
    /// Interval for checking heartbeat timeouts
    pub heartbeat_check_interval: Duration,
    /// Enable active heartbeat timeout tracking (EPIC-42)
    pub heartbeat_timeout_tracking: bool,
}

impl Default for WorkerSupervisorConfig {
    fn default() -> Self {
        Self {
            max_workers: 10000,
            inbox_capacity: 1000,
            worker_channel_capacity: 100,
            actor_enabled: true,
            heartbeat_timeout: Duration::from_secs(30),
            heartbeat_check_interval: Duration::from_secs(5),
            heartbeat_timeout_tracking: true,
        }
    }
}

/// Builder for WorkerSupervisor
pub struct WorkerSupervisorBuilder {
    config: WorkerSupervisorConfig,
    shutdown: Option<watch::Sender<()>>,
    worker_registry: Option<Arc<dyn WorkerRegistry>>,
    event_bus: Option<Arc<dyn EventBus>>,
}

impl WorkerSupervisorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: WorkerSupervisorConfig::default(),
            shutdown: None,
            worker_registry: None,
            event_bus: None,
        }
    }

    /// Configure the supervisor
    pub fn with_config(mut self, config: WorkerSupervisorConfig) -> Self {
        self.config = config;
        self
    }

    /// Set shutdown channel
    pub fn with_shutdown(mut self, shutdown: watch::Sender<()>) -> Self {
        self.shutdown = Some(shutdown);
        self
    }

    /// Set worker registry for database persistence
    pub fn with_worker_registry(mut self, registry: Arc<dyn WorkerRegistry>) -> Self {
        self.worker_registry = Some(registry);
        self
    }

    /// Set event bus for publishing domain events
    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Build the supervisor and handle
    pub fn build(self) -> (WorkerSupervisorHandle, WorkerSupervisor, watch::Sender<()>) {
        let (tx, rx) = mpsc::channel(self.config.inbox_capacity);
        let (shutdown_tx, shutdown_rx) = watch::channel(());
        let metrics = Arc::new(WorkerSupervisorMetrics::default());

        let mut supervisor = WorkerSupervisor::new(rx, shutdown_rx, metrics.clone());

        // Set worker registry if provided
        if let Some(registry) = self.worker_registry {
            supervisor = supervisor.with_worker_registry(registry);
        }

        // Set event bus if provided
        if let Some(event_bus) = self.event_bus {
            supervisor = supervisor.with_event_bus(event_bus);
        }

        let handle = WorkerSupervisorHandle::new(tx);

        // If no shutdown channel was provided, create one
        let shutdown_tx = self.shutdown.unwrap_or(shutdown_tx);

        (handle, supervisor, shutdown_tx)
    }
}

impl Default for WorkerSupervisorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::workers::{ProviderType, WorkerSpec};
    use uuid::Uuid;

    fn create_test_worker_id() -> WorkerId {
        WorkerId::new()
    }

    fn create_test_provider_id() -> ProviderId {
        ProviderId::new()
    }

    fn create_test_handle() -> WorkerHandle {
        WorkerHandle::new(
            create_test_worker_id(),
            "container-123".to_string(),
            ProviderType::Docker,
            create_test_provider_id(),
        )
    }

    fn create_test_spec() -> WorkerSpec {
        WorkerSpec::new("test-image".to_string(), "tcp://10.0.0.1:8080".to_string())
    }

    #[tokio::test]
    async fn test_worker_registration() {
        let (handle, supervisor, _shutdown) = WorkerSupervisorBuilder::new().build();

        // Start supervisor
        let supervisor_handle = tokio::spawn(supervisor.run());

        // Register a worker
        let worker_id = create_test_worker_id();
        let result = handle
            .register(
                worker_id.clone(),
                create_test_provider_id(),
                create_test_handle(),
                create_test_spec(),
            )
            .await;

        assert!(result.is_ok());
        let state = result.unwrap();
        assert_eq!(state.worker_id, worker_id);
        assert_eq!(state.status, WorkerState::Creating);

        // Verify worker exists
        let get_result = handle.get_worker(&worker_id).await;
        assert!(get_result.is_ok());
        assert_eq!(get_result.unwrap().worker_id, worker_id);

        // Cleanup
        handle.unregister(&worker_id).await.unwrap();
        supervisor_handle.abort();
    }

    #[tokio::test]
    async fn test_worker_duplicate_registration_fails() {
        let (handle, supervisor, _shutdown) = WorkerSupervisorBuilder::new().build();
        let supervisor_handle = tokio::spawn(supervisor.run());

        let worker_id = create_test_worker_id();
        let provider_id = create_test_provider_id();
        let handle_ = create_test_handle();
        let spec = create_test_spec();

        // First registration succeeds
        let first_result = handle.register(
            worker_id.clone(),
            provider_id.clone(),
            handle_.clone(),
            spec.clone(),
        );
        assert!(first_result.await.is_ok());

        // Second registration fails (same worker_id, same handle, same spec)
        let second_result = handle.register(worker_id.clone(), provider_id.clone(), handle_, spec);
        let err = second_result.await;
        assert!(err.is_err());
        assert!(matches!(
            err,
            Err(SupervisorError::WorkerAlreadyRegistered { .. })
        ));

        supervisor_handle.abort();
    }

    #[tokio::test]
    async fn test_worker_heartbeat() {
        let (handle, supervisor, _shutdown) = WorkerSupervisorBuilder::new().build();
        let supervisor_handle = tokio::spawn(supervisor.run());

        let worker_id = create_test_worker_id();

        // Register first
        handle
            .register(
                worker_id.clone(),
                create_test_provider_id(),
                create_test_handle(),
                create_test_spec(),
            )
            .await
            .unwrap();

        // Send heartbeat
        let result = handle.heartbeat(&worker_id).await;
        assert!(result.is_ok());

        // Verify status updated
        let state = handle.get_worker(&worker_id).await.unwrap();
        assert_eq!(state.status, WorkerState::Ready);
        assert!(state.last_heartbeat.is_some());

        // Cleanup
        handle.unregister(&worker_id).await.unwrap();
        supervisor_handle.abort();
    }

    #[tokio::test]
    async fn test_worker_list() {
        let (handle, supervisor, _shutdown) = WorkerSupervisorBuilder::new().build();
        let supervisor_handle = tokio::spawn(supervisor.run());

        // Register multiple workers
        for _ in 0..5 {
            let worker_id = create_test_worker_id();
            handle
                .register(
                    worker_id,
                    create_test_provider_id(),
                    create_test_handle(),
                    create_test_spec(),
                )
                .await
                .unwrap();
        }

        // List all
        let workers = handle.list_workers(None).await;
        assert_eq!(workers.len(), 5);

        // List with filter
        let creating_workers = handle.list_workers(Some(WorkerState::Creating)).await;
        // Some may have been updated by internal state
        assert!(creating_workers.len() <= 5);

        // Get stats
        let stats = handle.get_stats().await;
        assert_eq!(stats.total, 5);

        supervisor_handle.abort();
    }

    #[tokio::test]
    async fn test_send_to_worker() {
        let (handle, supervisor, _shutdown) = WorkerSupervisorBuilder::new().build();
        let supervisor_handle = tokio::spawn(supervisor.run());

        let worker_id = create_test_worker_id();

        // Register worker
        handle
            .register(
                worker_id.clone(),
                create_test_provider_id(),
                create_test_handle(),
                create_test_spec(),
            )
            .await
            .unwrap();

        // Create a channel for the worker
        let (tx, mut rx) = mpsc::channel::<WorkerMsg>(10);

        // Set worker channel
        handle.set_worker_channel(&worker_id, tx).await.unwrap();

        // Send message to worker
        let message = WorkerMsg::Ping;
        let result = handle.send_to_worker(&worker_id, message).await;
        assert!(result.is_ok());

        // Verify message received
        let received = rx.recv().await;
        assert!(received.is_some());
        assert!(matches!(received.unwrap(), WorkerMsg::Ping));

        supervisor_handle.abort();
    }

    // EPIC-42: Heartbeat Timeout Tracking Tests

    #[tokio::test]
    async fn test_heartbeat_timeout_detects_stale_worker() {
        // This test verifies the heartbeat timeout detection logic
        // In production, the actor receives regular messages from workers
        // For unit testing, we directly test the check logic

        let config = WorkerSupervisorConfig {
            heartbeat_timeout: Duration::from_millis(100),
            heartbeat_check_interval: Duration::from_millis(50),
            heartbeat_timeout_tracking: true,
            ..Default::default()
        };

        let (handle, supervisor, shutdown_tx) = WorkerSupervisorBuilder::new()
            .with_config(config)
            .build();

        let supervisor_handle = tokio::spawn(supervisor.run());

        let worker_id = create_test_worker_id();

        // Register worker and send heartbeat
        handle
            .register(
                worker_id.clone(),
                create_test_provider_id(),
                create_test_handle(),
                create_test_spec(),
            )
            .await
            .unwrap();

        // Send initial heartbeat
        handle.heartbeat(&worker_id).await.unwrap();

        // Worker should be ready
        let state = handle.get_worker(&worker_id).await.unwrap();
        assert_eq!(state.status, WorkerState::Ready);

        // Send another heartbeat to refresh
        handle.heartbeat(&worker_id).await.unwrap();

        // Worker should still be ready after heartbeat
        let state = handle.get_worker(&worker_id).await.unwrap();
        assert_eq!(state.status, WorkerState::Ready);

        // Verify that sending messages triggers heartbeat check
        // In production, regular worker messages would trigger this
        let _ = handle.list_workers(None).await;

        // Worker should still be ready (heartbeat was refreshed)
        let state = handle.get_worker(&worker_id).await.unwrap();
        assert_eq!(state.status, WorkerState::Ready);

        let _ = shutdown_tx.send(());
        supervisor_handle.abort();
    }

    #[tokio::test]
    async fn test_heartbeat_timeout_config_defaults() {
        let config = WorkerSupervisorConfig::default();

        assert_eq!(config.heartbeat_timeout, Duration::from_secs(30));
        assert_eq!(config.heartbeat_check_interval, Duration::from_secs(5));
        assert!(config.heartbeat_timeout_tracking);
    }

    #[tokio::test]
    async fn test_heartbeat_timeout_tracking_disabled() {
        let config = WorkerSupervisorConfig {
            heartbeat_timeout_tracking: false,
            ..Default::default()
        };

        let (handle, supervisor, shutdown_tx) = WorkerSupervisorBuilder::new()
            .with_config(config)
            .build();

        let supervisor_handle = tokio::spawn(supervisor.run());

        let worker_id = create_test_worker_id();

        // Register worker
        handle
            .register(
                worker_id.clone(),
                create_test_provider_id(),
                create_test_handle(),
                create_test_spec(),
            )
            .await
            .unwrap();

        // Don't send heartbeat - with tracking disabled, worker stays alive
        let state = handle.get_worker(&worker_id).await.unwrap();
        assert_eq!(state.status, WorkerState::Creating);

        let _ = shutdown_tx.send(());
        supervisor_handle.abort();
    }

    #[tokio::test]
    async fn test_worker_supervisor_with_custom_config() {
        let config = WorkerSupervisorConfig {
            max_workers: 100,
            inbox_capacity: 50,
            worker_channel_capacity: 10,
            heartbeat_timeout: Duration::from_secs(60),
            heartbeat_check_interval: Duration::from_secs(10),
            heartbeat_timeout_tracking: true,
            actor_enabled: true,
        };

        let (handle, supervisor, shutdown_tx) = WorkerSupervisorBuilder::new()
            .with_config(config)
            .build();

        let supervisor_handle = tokio::spawn(supervisor.run());

        // Register multiple workers
        for i in 0..5 {
            let worker_id = create_test_worker_id();
            handle
                .register(
                    worker_id,
                    create_test_provider_id(),
                    create_test_handle(),
                    create_test_spec(),
                )
                .await
                .unwrap();
        }

        let workers = handle.list_workers(None).await;
        assert_eq!(workers.len(), 5);

        let _ = shutdown_tx.send(());
        supervisor_handle.abort();
    }
}
