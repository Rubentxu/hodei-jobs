//! Saga Orchestrator
//!
//! Core orchestrator for executing sagas with automatic compensation.
//! Includes integrated audit trail for compliance and debugging.

use super::audit_trail::{SagaAuditConsumer, SagaAuditEntry, SagaAuditTrail};
use super::retry_policy::RetryPolicy;
use super::timeout_config::SagaTimeoutConfig;
use super::{
    CancellationSaga, CleanupSaga, ExecutionSaga, ProvisioningSaga, RecoverySaga, Saga,
    SagaContext, SagaError, SagaExecutionResult, SagaId, SagaOrchestrator, SagaRepository,
    SagaState, SagaType, TimeoutSaga,
};
use crate::shared_kernel::{DomainError, JobId, ProviderId, WorkerId};
use crate::workers::WorkerSpec;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Semaphore;

/// Rate limiter configuration
#[derive(Debug, Clone, Copy)]
pub struct RateLimitConfig {
    /// Maximum permits per window
    pub max_permits: usize,
    /// Window duration
    pub window: Duration,
    /// Tokens to add per interval
    pub refill_rate: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_permits: 100,
            window: Duration::from_secs(1),
            refill_rate: 100.0,
        }
    }
}

/// Token bucket rate limiter
#[derive(Debug, Clone)]
pub struct TokenBucketRateLimiter {
    permits: Arc<AtomicUsize>,
    max_permits: usize,
    refill_rate: f64,
    last_refill: Instant,
    window: Duration,
}

impl TokenBucketRateLimiter {
    /// Create a new token bucket rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            permits: Arc::new(AtomicUsize::new(config.max_permits)),
            max_permits: config.max_permits,
            refill_rate: config.refill_rate,
            last_refill: Instant::now(),
            window: config.window,
        }
    }

    /// Try to acquire a permit, returns true if acquired
    pub fn try_acquire(&self) -> bool {
        self.refill();
        self.permits
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                if current > 0 { Some(current - 1) } else { None }
            })
            .is_ok()
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let elapsed = self.last_refill.elapsed();
        if elapsed >= self.window {
            // Calculate how many tokens to add based on time elapsed
            let intervals = elapsed.as_secs_f64() / self.window.as_secs_f64();
            let to_add = (intervals * self.refill_rate) as usize;
            self.permits.fetch_min(self.max_permits, Ordering::SeqCst);
            self.permits
                .fetch_add(to_add.min(self.max_permits), Ordering::SeqCst);
            // Note: In a production implementation, we'd need to track this more precisely
        }
    }

    /// Get remaining permits
    pub fn remaining(&self) -> usize {
        self.refill();
        self.permits.load(Ordering::SeqCst).min(self.max_permits)
    }
}

/// Orchestrator configuration
#[derive(Debug, Clone)]
pub struct SagaOrchestratorConfig {
    pub max_concurrent_sagas: usize,
    pub max_concurrent_steps: usize,
    /// Default step timeout (can be overridden by saga type config)
    pub step_timeout: Duration,
    /// Default saga timeout (used when no type-specific config)
    pub saga_timeout: Duration,
    pub max_retries: u32,
    pub retry_backoff: Duration,
    /// Retry policy for compensation operations
    pub compensation_retry_policy: RetryPolicy,
    /// Rate limiting configuration
    pub rate_limit: Option<RateLimitConfig>,
    /// Maximum number of sagas per minute
    pub max_sagas_per_minute: Option<u32>,
    /// Type-specific timeout configuration (EPIC-85 US-03)
    pub saga_timeouts: SagaTimeoutConfig,
}

impl Default for SagaOrchestratorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sagas: 100,
            max_concurrent_steps: 10,
            step_timeout: Duration::from_secs(30),
            saga_timeout: Duration::from_secs(300),
            max_retries: 3,
            retry_backoff: Duration::from_secs(1),
            compensation_retry_policy: RetryPolicy {
                max_attempts: 3,                          // 1 initial + 2 retries
                initial_delay: Duration::from_millis(10), // Fast for tests
                max_delay: Duration::from_secs(60),
                multiplier: 2.0,
                jitter_factor: 0.2,
            },
            rate_limit: None,
            max_sagas_per_minute: Some(60),
            saga_timeouts: SagaTimeoutConfig::default(),
        }
    }
}

impl SagaOrchestratorConfig {
    /// Creates a configuration optimized for Kubernetes providers
    #[inline]
    pub fn kubernetes() -> Self {
        Self {
            saga_timeouts: SagaTimeoutConfig::kubernetes(),
            ..Default::default()
        }
    }

    /// Creates a configuration optimized for local Docker development
    #[inline]
    pub fn docker_local() -> Self {
        Self {
            saga_timeouts: SagaTimeoutConfig::docker_local(),
            ..Default::default()
        }
    }

    /// Creates a configuration optimized for Firecracker microVMs
    #[inline]
    pub fn firecracker() -> Self {
        Self {
            saga_timeouts: SagaTimeoutConfig::firecracker(),
            ..Default::default()
        }
    }

    /// Gets the saga-level timeout for a specific saga type
    #[inline]
    pub fn get_saga_timeout(&self, saga_type: SagaType) -> Duration {
        self.saga_timeouts.get_timeout(saga_type)
    }

    /// Gets the step-level timeout for a specific saga type
    /// Uses saga-specific step timeout if configured, otherwise default
    #[inline]
    pub fn get_step_timeout(&self, _saga_type: SagaType) -> Duration {
        // For now, we use a consistent step timeout
        // In the future, this could be configurable per saga type
        self.step_timeout
    }
}

/// Errors from orchestrator operations
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("Saga execution timeout after {duration:?}")]
    Timeout { duration: Duration },

    #[error("Saga was cancelled")]
    Cancelled,

    #[error("Saga step '{step}' failed: {error}")]
    StepFailed { step: String, error: String },

    #[error("Compensation failed for step '{step}': {error}")]
    CompensationFailed { step: String, error: String },

    #[error("Concurrency limit exceeded: {current}/{max}")]
    ConcurrencyLimitExceeded { current: usize, max: usize },

    #[error("Rate limit exceeded: {remaining} permits remaining, try again later")]
    RateLimitExceeded { remaining: usize },

    #[error("Saga not found: {saga_id}")]
    SagaNotFound { saga_id: SagaId },

    #[error("Persistence error: {message}")]
    PersistenceError { message: String },
}

impl From<SagaError> for OrchestratorError {
    fn from(err: SagaError) -> Self {
        match err {
            SagaError::Timeout { duration } => OrchestratorError::Timeout { duration },
            SagaError::Cancelled => OrchestratorError::Cancelled,
            SagaError::StepFailed { step, message, .. } => OrchestratorError::StepFailed {
                step,
                error: message,
            },
            SagaError::CompensationFailed { step, message } => {
                OrchestratorError::CompensationFailed {
                    step,
                    error: message,
                }
            }
            SagaError::SerializationError(msg) | SagaError::DeserializationError(msg) => {
                OrchestratorError::PersistenceError { message: msg }
            }
            _ => OrchestratorError::PersistenceError {
                message: err.to_string(),
            },
        }
    }
}

/// In-memory saga orchestrator for testing and development
///
/// This implementation stores saga state in memory and should not be
/// used in production environments where persistence is required.
#[derive(Debug, Clone)]
pub struct InMemorySagaOrchestrator<R: SagaRepository + Clone> {
    /// Repository for saga persistence
    repository: Arc<R>,
    /// Configuration
    config: SagaOrchestratorConfig,
    /// Active saga count (for concurrency control)
    active_sagas: Arc<AtomicUsize>,
    /// Rate limiter for saga execution
    rate_limiter: Option<TokenBucketRateLimiter>,
    /// Semaphore for concurrent saga control
    concurrency_semaphore: Arc<Semaphore>,
    /// Audit trail for this orchestrator (US-15)
    audit_trail: Arc<SagaAuditTrail>,
    /// Audit consumers for dispatching audit events
    audit_consumers: Vec<Arc<dyn SagaAuditConsumer>>,
}

impl<R: SagaRepository + Clone> InMemorySagaOrchestrator<R> {
    /// Creates a new in-memory orchestrator
    pub fn new(repository: Arc<R>, config: Option<SagaOrchestratorConfig>) -> Self {
        let config = config.unwrap_or_default();
        let rate_limiter = config.rate_limit.map(TokenBucketRateLimiter::new);
        let concurrency_semaphore = Arc::new(Semaphore::new(config.max_concurrent_sagas));

        Self {
            repository,
            config,
            active_sagas: Arc::new(AtomicUsize::new(0)),
            rate_limiter,
            concurrency_semaphore,
            audit_trail: Arc::new(SagaAuditTrail::new()),
            audit_consumers: Vec::new(),
        }
    }

    /// Create with custom audit consumers (US-15)
    pub fn with_audit_consumers(
        repository: Arc<R>,
        config: Option<SagaOrchestratorConfig>,
        consumers: Vec<Arc<dyn SagaAuditConsumer>>,
    ) -> Self {
        let mut orchestrator = Self::new(repository, config);
        orchestrator.audit_consumers = consumers;
        orchestrator
    }

    /// Get the audit trail
    pub fn audit_trail(&self) -> &Arc<SagaAuditTrail> {
        &self.audit_trail
    }

    /// Record an audit entry and dispatch to consumers (US-15)
    fn record_audit(&self, entry: SagaAuditEntry) {
        // Record in local trail
        self.audit_trail.record(entry.clone());
        // Dispatch to all consumers
        for consumer in &self.audit_consumers {
            consumer.consume(&entry);
        }
    }

    /// Execute a single compensation step with retry using exponential backoff
    async fn execute_compensation_with_retry(
        &self,
        step: &dyn super::SagaStep<Output = ()>,
        context: &mut SagaContext,
        step_name: &str,
        step_idx: usize,
        saga_type: SagaType,
        saga_id: &SagaId,
    ) -> Result<bool, SagaError> {
        let policy = self.config.compensation_retry_policy;
        let mut attempt = 0u32;

        // Record compensation started (US-15)
        self.record_audit(
            SagaAuditEntry::new(
                saga_id.clone(),
                saga_type,
                super::audit_trail::SagaAuditEventType::CompensationStarted,
            )
            .with_step(step_name, step_idx)
            .with_attempt(0),
        );

        loop {
            attempt += 1;
            let comp_start = std::time::Instant::now();
            match step.compensate(context).await {
                Ok(()) => {
                    // Record compensation completed (US-15)
                    let comp_duration = comp_start.elapsed();
                    self.record_audit(
                        SagaAuditEntry::new(
                            saga_id.clone(),
                            saga_type,
                            super::audit_trail::SagaAuditEventType::CompensationCompleted,
                        )
                        .with_step(step_name, step_idx)
                        .with_attempt(attempt)
                        .with_duration(comp_duration),
                    );
                    return Ok(true);
                }
                Err(e) => {
                    // Record compensation failed (US-15)
                    self.record_audit(
                        SagaAuditEntry::new(
                            saga_id.clone(),
                            saga_type,
                            super::audit_trail::SagaAuditEventType::CompensationFailed,
                        )
                        .with_step(step_name, step_idx)
                        .with_attempt(attempt)
                        .with_error(&e.to_string()),
                    );

                    if !policy.should_retry(attempt) {
                        tracing::error!(
                            step = step_name,
                            attempt = attempt,
                            max_attempts = policy.max_attempts(),
                            error = %e,
                            "Compensation step failed after all retries"
                        );
                        return Err(e);
                    }

                    // Calculate delay with exponential backoff
                    let delay = policy.delay_for_attempt(attempt);
                    tracing::warn!(
                        step = step_name,
                        attempt = attempt,
                        max_attempts = policy.max_attempts(),
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "Compensation step failed, retrying with exponential backoff"
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Execute all compensation steps in reverse order with retry
    async fn execute_compensation(
        &self,
        steps: &[Box<dyn super::SagaStep<Output = ()>>],
        context: &mut SagaContext,
        executed_steps_count: usize,
        saga_type: SagaType,
        saga_id: &SagaId,
    ) -> u32 {
        let mut compensations_executed = 0;

        for compensate_idx in (0..executed_steps_count).rev() {
            let compensate_step = &steps[compensate_idx];

            if compensate_step.has_compensation() {
                let step_name = compensate_step.name();

                match self
                    .execute_compensation_with_retry(
                        &**compensate_step,
                        context,
                        step_name,
                        compensate_idx,
                        saga_type,
                        saga_id,
                    )
                    .await
                {
                    Ok(_) => {
                        compensations_executed += 1;
                        tracing::info!(step = step_name, "Compensation step executed successfully");
                    }
                    Err(comp_error) => {
                        tracing::error!(
                            step = step_name,
                            error = %comp_error,
                            "Compensation step failed"
                        );
                        // Continue compensating other steps even if one fails
                    }
                }
            }
        }

        compensations_executed
    }
}

#[async_trait::async_trait]
impl<R: SagaRepository + Clone + Send + Sync + 'static> SagaOrchestrator
    for InMemorySagaOrchestrator<R>
where
    <R as SagaRepository>::Error: std::fmt::Display + Send + Sync,
{
    type Error = DomainError;

    async fn execute_saga(
        &self,
        saga: &dyn Saga,
        mut context: SagaContext,
    ) -> Result<SagaExecutionResult, Self::Error> {
        let start_time = std::time::Instant::now();
        let saga_id = context.saga_id.clone();
        let saga_type = saga.saga_type();

        // Record saga started event (US-15)
        self.record_audit(SagaAuditEntry::new(
            saga_id.clone(),
            saga_type,
            super::audit_trail::SagaAuditEventType::Started,
        ));

        // Check rate limit first
        if let Some(ref limiter) = self.rate_limiter
            && !limiter.try_acquire() {
                let remaining = limiter.remaining();
                return Err(DomainError::OrchestratorError(
                    OrchestratorError::RateLimitExceeded { remaining },
                ));
            }

        // Try to acquire semaphore permit for concurrency control
        let _permit = self.concurrency_semaphore.try_acquire().map_err(|_| {
            DomainError::OrchestratorError(OrchestratorError::ConcurrencyLimitExceeded {
                current: self.active_sagas.load(Ordering::SeqCst),
                max: self.config.max_concurrent_sagas,
            })
        })?;

        // Increment active count
        self.active_sagas.fetch_add(1, Ordering::SeqCst);

        // Ensure we decrement on exit (using a simple guard)
        let active_sagas = self.active_sagas.clone();
        let _guard = DropGuard(active_sagas);

        // Save initial context
        self.repository
            .save(&context)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;

        let steps = saga.steps();
        let mut executed_steps = context.current_step;
        let is_resume = context.current_step > 0;
        let saga_type = saga.saga_type();

        // Get type-specific timeouts (EPIC-85 US-03)
        let saga_timeout = self.config.get_saga_timeout(saga_type);
        let step_timeout = self.config.get_step_timeout(saga_type);

        // Execute steps sequentially
        let steps_vec = steps.into_iter().collect::<Vec<_>>();

        // Log resume status
        if is_resume {
            tracing::info!(
                saga_id = %saga_id,
                saga_type = ?saga_type,
                current_step = context.current_step,
                saga_timeout_secs = saga_timeout.as_secs(),
                "Resuming saga from step"
            );
        }

        for (step_idx, step) in steps_vec.iter().enumerate() {
            // Skip steps that were already executed (resume scenario)
            if step_idx < context.current_step {
                tracing::debug!(
                    step = step.name(),
                    step_idx = step_idx,
                    "Skipping already executed step (resume)"
                );
                continue;
            }

            // Record step started (US-15)
            let step_start = std::time::Instant::now();
            self.record_audit(
                SagaAuditEntry::new(
                    saga_id.clone(),
                    saga_type,
                    super::audit_trail::SagaAuditEventType::StepStarted,
                )
                .with_step(step.name(), step_idx),
            );

            // Check saga timeout using type-specific timeout
            if start_time.elapsed() > saga_timeout {
                context.set_error(format!("Saga timed out after {:?}", start_time.elapsed()));
                self.repository
                    .update_state(&saga_id, SagaState::Failed, context.error_message.clone())
                    .await
                    .ok();

                return Err(DomainError::SagaTimeout {
                    duration: start_time.elapsed(),
                });
            }

            // Execute step with type-specific timeout
            let step_result = tokio::time::timeout(step_timeout, step.execute(&mut context)).await;

            match step_result {
                Ok(Ok(_output)) => {
                    // Step succeeded - record audit (US-15)
                    let step_duration = step_start.elapsed();
                    self.record_audit(
                        SagaAuditEntry::new(
                            saga_id.clone(),
                            saga_type,
                            super::audit_trail::SagaAuditEventType::StepCompleted,
                        )
                        .with_step(step.name(), step_idx)
                        .with_duration(step_duration),
                    );
                    executed_steps += 1;
                }
                Ok(Err(e)) => {
                    // Step failed - record audit (US-15)
                    self.record_audit(
                        SagaAuditEntry::new(
                            saga_id.clone(),
                            saga_type,
                            super::audit_trail::SagaAuditEventType::StepFailed,
                        )
                        .with_step(step.name(), step_idx)
                        .with_error(&e.to_string())
                        .with_compensation(true),
                    );
                    // Step failed - start REAL compensation in REVERSE order
                    context.set_error(e.to_string());
                    context.is_compensating = true;

                    self.repository
                        .update_state(&saga_id, SagaState::Compensating, Some(e.to_string()))
                        .await
                        .ok();

                    // Execute compensation for all previously executed steps in REVERSE order with retry
                    let compensations_executed = self
                        .execute_compensation(
                            &steps_vec,
                            &mut context,
                            step_idx,
                            saga_type,
                            &saga_id,
                        )
                        .await;

                    // Record saga failed event (US-15)
                    self.record_audit(
                        SagaAuditEntry::new(
                            saga_id.clone(),
                            saga_type,
                            super::audit_trail::SagaAuditEventType::Failed,
                        )
                        .with_error(&e.to_string())
                        .with_metadata("steps_executed", serde_json::json!(executed_steps))
                        .with_metadata(
                            "compensations_executed",
                            serde_json::json!(compensations_executed),
                        ),
                    );

                    // Update final state to Failed (compensation completed but saga failed)
                    self.repository
                        .update_state(&saga_id, SagaState::Failed, Some(e.to_string()))
                        .await
                        .ok();

                    return Ok(SagaExecutionResult::failed(
                        saga_id,
                        saga.saga_type(),
                        start_time.elapsed(),
                        executed_steps as u32,
                        compensations_executed,
                        e.to_string(),
                    ));
                }
                Err(_) => {
                    // Step timed out
                    let timeout_error =
                        format!("Step timed out after {:?}", self.config.step_timeout);
                    context.set_error(timeout_error.clone());
                    context.is_compensating = true;

                    self.repository
                        .update_state(
                            &saga_id,
                            SagaState::Compensating,
                            Some(timeout_error.clone()),
                        )
                        .await
                        .ok();

                    // Execute compensation for all previously executed steps in REVERSE order with retry
                    let compensations_executed = self
                        .execute_compensation(
                            &steps_vec,
                            &mut context,
                            executed_steps,
                            saga_type,
                            &saga_id,
                        )
                        .await;

                    // Record saga timed out event (US-15)
                    self.record_audit(
                        SagaAuditEntry::new(
                            saga_id.clone(),
                            saga_type,
                            super::audit_trail::SagaAuditEventType::TimedOut,
                        )
                        .with_error(&timeout_error)
                        .with_metadata("steps_executed", serde_json::json!(executed_steps))
                        .with_metadata(
                            "compensations_executed",
                            serde_json::json!(compensations_executed),
                        ),
                    );

                    self.repository
                        .update_state(&saga_id, SagaState::Failed, Some(timeout_error))
                        .await
                        .ok();

                    return Err(DomainError::SagaTimeout {
                        duration: self.config.step_timeout,
                    });
                }
            }
        }

        // All steps completed successfully - record completion (US-15)
        let total_duration = start_time.elapsed();
        self.record_audit(
            SagaAuditEntry::new(
                saga_id.clone(),
                saga_type,
                super::audit_trail::SagaAuditEventType::Completed,
            )
            .with_duration(total_duration)
            .with_metadata("steps_executed", serde_json::json!(executed_steps)),
        );

        self.repository
            .update_state(&saga_id, SagaState::Completed, None)
            .await
            .ok();

        Ok(SagaExecutionResult::completed_with_steps(
            saga_id,
            saga.saga_type(),
            total_duration,
            executed_steps as u32,
        ))
    }

    /// EPIC-42: Execute saga directly from context (for reactive processing)
    async fn execute(&self, context: &SagaContext) -> Result<SagaExecutionResult, Self::Error> {
        // Create the appropriate saga based on saga_type
        let saga: Box<dyn Saga> = match context.saga_type {
            SagaType::Provisioning => {
                // Extract provider_id from metadata
                let provider_id_str = context
                    .metadata
                    .get("provider_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let provider_id = if !provider_id_str.is_empty() {
                    ProviderId::from_uuid(
                        uuid::Uuid::parse_str(&provider_id_str)
                            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    ProviderId::new()
                };

                let spec = WorkerSpec::new(
                    "hodei-jobs-worker:latest".to_string(),
                    "http://localhost:50051".to_string(),
                );

                Box::new(ProvisioningSaga::new(spec, provider_id))
            }
            SagaType::Execution => {
                let job_id_str = context
                    .metadata
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let job_id = if !job_id_str.is_empty() {
                    JobId(
                        uuid::Uuid::parse_str(&job_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    JobId::new()
                };

                Box::new(ExecutionSaga::new(job_id))
            }
            SagaType::Recovery => Box::new(RecoverySaga::new(JobId::new(), WorkerId::new(), None)),
            // EPIC-50 GAP-MOD-01: Implement Cancellation, Timeout, and Cleanup saga types
            SagaType::Cancellation => {
                let job_id_str = context
                    .metadata
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let job_id = if !job_id_str.is_empty() {
                    JobId(
                        uuid::Uuid::parse_str(&job_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    return Err(DomainError::InfrastructureError {
                        message: "job_id required for CancellationSaga".to_string(),
                    });
                };

                let reason = context
                    .metadata
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("User requested")
                    .to_string();

                Box::new(CancellationSaga::new(job_id, reason))
            }
            SagaType::Timeout => {
                let job_id_str = context
                    .metadata
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let job_id = if !job_id_str.is_empty() {
                    JobId(
                        uuid::Uuid::parse_str(&job_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    return Err(DomainError::InfrastructureError {
                        message: "job_id required for TimeoutSaga".to_string(),
                    });
                };

                let timeout_secs = context
                    .metadata
                    .get("timeout_secs")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(300) as u64;

                let reason = context
                    .metadata
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("timeout_exceeded")
                    .to_string();

                Box::new(TimeoutSaga::new(
                    job_id,
                    Duration::from_secs(timeout_secs),
                    reason,
                ))
            }
            SagaType::Cleanup => {
                let dry_run = context
                    .metadata
                    .get("dry_run")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let unhealthy_threshold_secs = context
                    .metadata
                    .get("unhealthy_threshold_secs")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(300) as u64;

                let orphaned_threshold_secs = context
                    .metadata
                    .get("orphaned_threshold_secs")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(600) as u64;

                Box::new(
                    CleanupSaga::with_thresholds(
                        Duration::from_secs(unhealthy_threshold_secs),
                        Duration::from_secs(orphaned_threshold_secs),
                    )
                    .with_dry_run(dry_run),
                )
            }
        };

        // Execute with a clone of the context
        self.execute_saga(&*saga, context.clone()).await
    }

    async fn get_saga(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
        self.repository
            .find_by_id(saga_id)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })
    }

    async fn cancel_saga(&self, saga_id: &SagaId) -> Result<(), Self::Error> {
        self.repository
            .mark_compensating(saga_id)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;

        self.repository
            .update_state(
                saga_id,
                SagaState::Cancelled,
                Some("Cancelled by user".to_string()),
            )
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;

        Ok(())
    }
}

/// Simple guard to decrement active saga count
struct DropGuard(Arc<AtomicUsize>);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::{
        SagaContext, SagaError, SagaResult, SagaStep, SagaStepData, SagaStepId, SagaStepState,
        SagaType,
    };
    use chrono::Utc;

    // Test saga implementation - uses Rc<RefCell> to allow cloning
    struct TestSaga {
        success_count: usize,
        fail_on_step: Option<usize>,
    }

    impl TestSaga {
        fn new(success_count: usize, fail_on_step: Option<usize>) -> Self {
            Self {
                success_count,
                fail_on_step,
            }
        }
    }

    impl Saga for TestSaga {
        fn saga_type(&self) -> SagaType {
            SagaType::Execution
        }

        fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
            let mut steps: Vec<Box<dyn SagaStep<Output = ()>>> = Vec::new();
            for i in 0..self.success_count {
                let fail_on = self.fail_on_step;
                steps.push(Box::new(TestStep {
                    step_number: i,
                    should_fail: fail_on == Some(i),
                }));
            }
            steps
        }
    }

    struct TestStep {
        step_number: usize,
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl SagaStep for TestStep {
        type Output = ();

        fn name(&self) -> &'static str {
            if self.should_fail {
                "FailStep"
            } else {
                "SuccessStep"
            }
        }

        async fn execute(&self, _context: &mut SagaContext) -> Result<(), SagaError> {
            if self.should_fail {
                Err(SagaError::StepFailed {
                    step: format!("Step{}", self.step_number),
                    message: "Intentional failure".to_string(),
                    will_compensate: true,
                })
            } else {
                Ok(())
            }
        }

        async fn compensate(&self, _context: &mut SagaContext) -> Result<(), SagaError> {
            Ok(())
        }
    }

    // Mock repository for testing
    #[derive(Clone)]
    struct MockSagaRepository;

    #[async_trait::async_trait]
    impl SagaRepository for MockSagaRepository {
        type Error = std::io::Error;

        async fn save(&self, _context: &SagaContext) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn create_if_not_exists(&self, _context: &SagaContext) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn find_by_id(&self, _saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
            Ok(None)
        }

        async fn find_by_type(
            &self,
            _saga_type: SagaType,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }

        async fn find_by_state(&self, _state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }

        async fn find_by_correlation_id(
            &self,
            _correlation_id: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }

        async fn update_state(
            &self,
            _saga_id: &SagaId,
            _state: SagaState,
            _error_message: Option<String>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn mark_compensating(&self, _saga_id: &SagaId) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn delete(&self, _saga_id: &SagaId) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn save_step(&self, _step: &SagaStepData) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn find_step_by_id(
            &self,
            _step_id: &SagaStepId,
        ) -> Result<Option<SagaStepData>, Self::Error> {
            Ok(None)
        }

        async fn find_steps_by_saga_id(
            &self,
            _saga_id: &SagaId,
        ) -> Result<Vec<SagaStepData>, Self::Error> {
            Ok(Vec::new())
        }

        async fn update_step_state(
            &self,
            _step_id: &SagaStepId,
            _state: SagaStepState,
            _output: Option<serde_json::Value>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn update_step_compensation(
            &self,
            _step_id: &SagaStepId,
            _compensation_data: serde_json::Value,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn count_active(&self) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn count_by_type_and_state(
            &self,
            _saga_type: SagaType,
            _state: SagaState,
        ) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn avg_duration(&self) -> Result<Option<Duration>, Self::Error> {
            Ok(None)
        }

        async fn cleanup_completed(&self, _older_than: Duration) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn claim_pending_sagas(
            &self,
            _limit: u64,
            _instance_id: &str,
        ) -> Result<Vec<SagaContext>, Self::Error> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_execute_saga_completes() {
        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // Create saga with 3 successful steps
        let saga = TestSaga::new(3, None);

        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_success());
        assert_eq!(saga_result.steps_executed, 3);
        assert_eq!(saga_result.compensations_executed, 0);
    }

    #[tokio::test]
    async fn test_execute_saga_compensates_on_failure() {
        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // Create saga with 2 steps where step 1 fails
        let saga = TestSaga::new(2, Some(1));

        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed() || saga_result.is_compensated());
        assert_eq!(saga_result.steps_executed, 1);
    }

    // ============ REAL COMPENSATION TESTS ============

    use std::sync::Mutex;

    #[tokio::test]
    async fn test_compensation_loop_executes_in_reverse_order() {
        // Use Mutex<Vec<usize>> to track which steps were compensated
        let compensated_steps: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        // Step that tracks when compensate() is called
        struct TrackingCompensatingStep {
            step_number: usize,
            should_fail: bool,
            compensated: Arc<Mutex<Vec<usize>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for TrackingCompensatingStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "TrackingCompensatingStep"
            }

            fn has_compensation(&self) -> bool {
                true
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                if self.should_fail {
                    Err(SagaError::StepFailed {
                        step: format!("Step{}", self.step_number),
                        message: "Intentional failure".to_string(),
                        will_compensate: true,
                    })
                } else {
                    Ok(())
                }
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                let mut compensated = self.compensated.lock().unwrap();
                compensated.push(self.step_number);
                Ok(())
            }
        }

        // Create a saga with 3 steps where step 2 fails
        struct TestSaga {
            fail_on_step: usize,
            compensated: Arc<Mutex<Vec<usize>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let compensated = self.compensated.clone();
                vec![
                    Box::new(TrackingCompensatingStep {
                        step_number: 0,
                        should_fail: false,
                        compensated: compensated.clone(),
                    }),
                    Box::new(TrackingCompensatingStep {
                        step_number: 1,
                        should_fail: false,
                        compensated: compensated.clone(),
                    }),
                    Box::new(TrackingCompensatingStep {
                        step_number: 2,
                        should_fail: self.fail_on_step == 2,
                        compensated: compensated.clone(),
                    }),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        let saga = TestSaga {
            fail_on_step: 2,
            compensated: compensated_steps.clone(),
        };
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed());

        // Steps 0 and 1 should have been compensated
        let compensated = compensated_steps.lock().unwrap();
        assert_eq!(compensated.len(), 2);
        assert!(compensated.contains(&0));
        assert!(compensated.contains(&1));
    }

    #[tokio::test]
    async fn test_compensation_skips_steps_without_compensation() {
        // Step with no compensation defined
        struct NoCompensationTestStep;

        #[async_trait::async_trait]
        impl SagaStep for NoCompensationTestStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "NoCompensationTestStep"
            }

            fn has_compensation(&self) -> bool {
                false
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                panic!("Should not be called!");
            }
        }

        // Step that fails during execution
        struct FailingTestStep;

        #[async_trait::async_trait]
        impl SagaStep for FailingTestStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "FailingTestStep"
            }

            fn has_compensation(&self) -> bool {
                true
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Err(SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "Intentional failure".to_string(),
                    will_compensate: true,
                })
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga;

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![
                    Box::new(NoCompensationTestStep),
                    Box::new(FailingTestStep),
                    Box::new(NoCompensationTestStep),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        let saga = TestSaga;
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed());

        // FailingTestStep has compensation, but it failed execution, so nothing to compensate
        assert_eq!(saga_result.compensations_executed, 0);
    }

    #[tokio::test]
    async fn test_compensation_with_exponential_backoff_retry() {
        // Test that verifies compensation uses retry with exponential backoff
        let attempts_0: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
        let attempts_1: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));

        struct RetryableCompensationStep {
            step_number: usize,
            fail_until_attempt: u32,
            attempts: Arc<Mutex<Vec<u32>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for RetryableCompensationStep {
            type Output = ();
            fn name(&self) -> &'static str {
                "RetryableCompensationStep"
            }
            fn has_compensation(&self) -> bool {
                true
            }
            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                let count = self.attempts.lock().unwrap().len() as u32 + 1;
                self.attempts.lock().unwrap().push(count);
                if count < self.fail_until_attempt {
                    Err(SagaError::CompensationFailed {
                        step: self.name().to_string(),
                        message: format!("Intentional failure on attempt {}", count),
                    })
                } else {
                    Ok(())
                }
            }
        }

        struct FailingExecuteStep;
        #[async_trait::async_trait]
        impl SagaStep for FailingExecuteStep {
            type Output = ();
            fn name(&self) -> &'static str {
                "FailingExecuteStep"
            }
            fn has_compensation(&self) -> bool {
                true
            }
            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Err(SagaError::StepFailed {
                    step: self.name().to_string(),
                    message: "Intentional failure".to_string(),
                    will_compensate: true,
                })
            }
            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga {
            attempts_0: Arc<Mutex<Vec<u32>>>,
            attempts_1: Arc<Mutex<Vec<u32>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }
            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![
                    Box::new(RetryableCompensationStep {
                        step_number: 0,
                        fail_until_attempt: 2,
                        attempts: self.attempts_0.clone(),
                    }),
                    Box::new(RetryableCompensationStep {
                        step_number: 1,
                        fail_until_attempt: 3,
                        attempts: self.attempts_1.clone(),
                    }),
                    Box::new(FailingExecuteStep),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let config = SagaOrchestratorConfig {
            compensation_retry_policy: RetryPolicy {
                max_attempts: 3,
                initial_delay: Duration::from_millis(10),
                max_delay: Duration::from_secs(60),
                multiplier: 2.0,
                jitter_factor: 0.2,
            },
            ..Default::default()
        };
        let orchestrator = InMemorySagaOrchestrator::new(repo, Some(config));

        let saga = TestSaga {
            attempts_0: attempts_0.clone(),
            attempts_1: attempts_1.clone(),
        };
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;
        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_failed());

        let attempts_0 = attempts_0.lock().unwrap();
        let attempts_1 = attempts_1.lock().unwrap();

        // Compensation happens in REVERSE order: step 1, then step 0
        assert_eq!(attempts_1.len(), 3, "Step 1 should have 3 attempts");
        assert_eq!(attempts_0.len(), 2, "Step 0 should have 2 attempts");
        assert_eq!(attempts_1[0], 1);
        assert_eq!(attempts_1[1], 2);
        assert_eq!(attempts_1[2], 3);
        assert_eq!(attempts_0[0], 1);
        assert_eq!(attempts_0[1], 2);
    }

    // ============ SAGA RESUME TESTS ============

    #[tokio::test]
    async fn test_saga_resumes_from_last_completed_step() {
        let executed_steps: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        struct TrackingStep {
            step_number: usize,
            executed: Arc<Mutex<Vec<usize>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for TrackingStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "TrackingStep"
            }

            fn has_compensation(&self) -> bool {
                false
            }

            async fn execute(&self, context: &mut SagaContext) -> SagaResult<()> {
                let mut executed = self.executed.lock().unwrap();
                executed.push(self.step_number);
                // Update context to track progress
                context.current_step = self.step_number + 1;
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga {
            executed: Arc<Mutex<Vec<usize>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let executed = self.executed.clone();
                vec![
                    Box::new(TrackingStep {
                        step_number: 0,
                        executed: executed.clone(),
                    }),
                    Box::new(TrackingStep {
                        step_number: 1,
                        executed: executed.clone(),
                    }),
                    Box::new(TrackingStep {
                        step_number: 2,
                        executed: executed.clone(),
                    }),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // First run: saga with 3 steps
        let saga = TestSaga {
            executed: executed_steps.clone(),
        };
        // Start from step 1 (simulating resume after step 0 completed)
        let saga_id = SagaId::new();
        let context = SagaContext::from_persistence(
            saga_id,
            SagaType::Execution,
            None,
            None,
            Utc::now(),
            1, // Step 0 already completed
            false,
            std::collections::HashMap::new(),
            None,
            SagaState::InProgress,
            1,
            None,
        );

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_success());

        // Only steps 1 and 2 should have been executed
        let executed = executed_steps.lock().unwrap();
        assert_eq!(executed.len(), 2);
        assert!(executed.contains(&1));
        assert!(executed.contains(&2));
        // Step 0 should NOT be executed again
        assert!(!executed.contains(&0));
    }

    #[tokio::test]
    async fn test_saga_starts_from_beginning_if_no_completed_steps() {
        let executed_steps: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        struct TrackingStep {
            step_number: usize,
            executed: Arc<Mutex<Vec<usize>>>,
        }

        #[async_trait::async_trait]
        impl SagaStep for TrackingStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "TrackingStep"
            }

            fn has_compensation(&self) -> bool {
                false
            }

            async fn execute(&self, context: &mut SagaContext) -> SagaResult<()> {
                let mut executed = self.executed.lock().unwrap();
                executed.push(self.step_number);
                context.current_step = self.step_number + 1;
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct TestSaga {
            executed: Arc<Mutex<Vec<usize>>>,
        }

        impl Saga for TestSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let executed = self.executed.clone();
                vec![
                    Box::new(TrackingStep {
                        step_number: 0,
                        executed: executed.clone(),
                    }),
                    Box::new(TrackingStep {
                        step_number: 1,
                        executed: executed.clone(),
                    }),
                ]
            }
        }

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, None);

        // Start from beginning (current_step = 0)
        let saga = TestSaga {
            executed: executed_steps.clone(),
        };
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;

        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_success());

        // All steps should have been executed
        let executed = executed_steps.lock().unwrap();
        assert_eq!(executed.len(), 2);
        assert!(executed.contains(&0));
        assert!(executed.contains(&1));
    }

    // ============ RATE LIMITING TESTS ============

    #[tokio::test]
    async fn test_token_bucket_rate_limiter() {
        let config = RateLimitConfig {
            max_permits: 3,
            window: Duration::from_secs(1),
            refill_rate: 1.0,
        };
        let limiter = TokenBucketRateLimiter::new(config);

        // Should allow first 3 requests
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert_eq!(limiter.remaining(), 0);

        // Fourth request should be denied
        assert!(!limiter.try_acquire());
    }

    #[tokio::test]
    async fn test_orchestrator_with_rate_limit() {
        let executed_count: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        struct CountingSaga {
            count: Arc<AtomicUsize>,
        }

        impl Saga for CountingSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }
            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                let count = self.count.clone();
                vec![Box::new(CountingStep { count })]
            }
        }

        struct CountingStep {
            count: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl SagaStep for CountingStep {
            type Output = ();
            fn name(&self) -> &'static str {
                "CountingStep"
            }
            fn has_compensation(&self) -> bool {
                false
            }
            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        // Configurar con rate limit muy restrictivo
        let config = SagaOrchestratorConfig {
            rate_limit: Some(RateLimitConfig {
                max_permits: 1,
                window: Duration::from_secs(1),
                refill_rate: 1.0,
            }),
            ..Default::default()
        };

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, Some(config));

        let saga = CountingSaga {
            count: executed_count.clone(),
        };
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        // Primera ejecucion debe funcionar
        let result = orchestrator.execute_saga(&saga, context).await;
        assert!(result.is_ok());
        assert_eq!(executed_count.load(Ordering::SeqCst), 1);

        // Segunda ejecucion inmediata debe fallar por rate limit
        let saga2 = CountingSaga {
            count: executed_count.clone(),
        };
        let context2 = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        let result2 = orchestrator.execute_saga(&saga2, context2).await;
        assert!(result2.is_err());
    }

    // ============ SAGA TIMEOUT CONFIG TESTS (EPIC-85 US-03) ============

    #[tokio::test]
    async fn test_saga_orchestrator_config_presets() {
        // Test Kubernetes preset
        let k8s_config = SagaOrchestratorConfig::kubernetes();
        assert_eq!(
            k8s_config.get_saga_timeout(SagaType::Provisioning),
            Duration::from_secs(600)
        );
        assert_eq!(
            k8s_config.get_saga_timeout(SagaType::Execution),
            Duration::from_secs(7200)
        );

        // Test Docker local preset
        let docker_config = SagaOrchestratorConfig::docker_local();
        assert_eq!(
            docker_config.get_saga_timeout(SagaType::Provisioning),
            Duration::from_secs(60)
        );
        assert_eq!(
            docker_config.get_saga_timeout(SagaType::Cancellation),
            Duration::from_secs(30)
        );

        // Test Firecracker preset
        let fc_config = SagaOrchestratorConfig::firecracker();
        assert_eq!(
            fc_config.get_saga_timeout(SagaType::Provisioning),
            Duration::from_secs(120)
        );
    }

    #[tokio::test]
    async fn test_get_timeout_returns_correct_value_for_all_saga_types() {
        let config = SagaOrchestratorConfig::default();

        // All saga types should return a valid timeout
        assert_eq!(
            config.get_saga_timeout(SagaType::Provisioning),
            Duration::from_secs(300)
        );
        assert_eq!(
            config.get_saga_timeout(SagaType::Execution),
            Duration::from_secs(7200)
        );
        assert_eq!(
            config.get_saga_timeout(SagaType::Recovery),
            Duration::from_secs(900)
        );
        assert_eq!(
            config.get_saga_timeout(SagaType::Cancellation),
            Duration::from_secs(120)
        );
        assert_eq!(
            config.get_saga_timeout(SagaType::Timeout),
            Duration::from_secs(120)
        );
        assert_eq!(
            config.get_saga_timeout(SagaType::Cleanup),
            Duration::from_secs(300)
        );
    }

    #[tokio::test]
    async fn test_different_saga_types_use_different_timeouts() {
        // Create custom config with very different timeouts
        let config = SagaOrchestratorConfig {
            saga_timeouts: SagaTimeoutConfig::new(
                Duration::from_secs(10), // provisioning: 10s
                Duration::from_secs(20), // execution: 20s
                Duration::from_secs(30), // recovery: 30s
                Duration::from_secs(5),  // cancellation: 5s
                Duration::from_secs(15), // timeout: 15s
                Duration::from_secs(25), // cleanup: 25s
            ),
            ..Default::default()
        };

        // Verify each type has distinct timeout
        assert_ne!(
            config.get_saga_timeout(SagaType::Provisioning),
            config.get_saga_timeout(SagaType::Execution)
        );
        assert_ne!(
            config.get_saga_timeout(SagaType::Execution),
            config.get_saga_timeout(SagaType::Recovery)
        );
        assert_ne!(
            config.get_saga_timeout(SagaType::Recovery),
            config.get_saga_timeout(SagaType::Cancellation)
        );
    }

    #[tokio::test]
    async fn test_execution_saga_uses_configured_timeout() {
        // Create a saga that takes longer than default timeout
        struct SlowStep;

        #[async_trait::async_trait]
        impl SagaStep for SlowStep {
            type Output = ();

            fn name(&self) -> &'static str {
                "SlowStep"
            }

            fn has_compensation(&self) -> bool {
                false
            }

            async fn execute(&self, _context: &mut SagaContext) -> SagaResult<()> {
                // Simulate slow operation
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(())
            }

            async fn compensate(&self, _context: &mut SagaContext) -> SagaResult<()> {
                Ok(())
            }
        }

        struct SlowSaga;

        impl Saga for SlowSaga {
            fn saga_type(&self) -> SagaType {
                SagaType::Execution
            }

            fn steps(&self) -> Vec<Box<dyn SagaStep<Output = ()>>> {
                vec![Box::new(SlowStep)]
            }
        }

        // Use execution timeout of 1 second (should succeed)
        let config = SagaOrchestratorConfig {
            saga_timeouts: SagaTimeoutConfig::new(
                Duration::from_secs(300),
                Duration::from_secs(1), // Short execution timeout
                Duration::from_secs(900),
                Duration::from_secs(120),
                Duration::from_secs(120),
                Duration::from_secs(300),
            ),
            ..Default::default()
        };

        let repo = Arc::new(MockSagaRepository);
        let orchestrator = InMemorySagaOrchestrator::new(repo, Some(config));

        let saga = SlowSaga;
        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        let result = orchestrator.execute_saga(&saga, context).await;
        assert!(result.is_ok());
        let saga_result = result.unwrap();
        assert!(saga_result.is_success());
    }
}
