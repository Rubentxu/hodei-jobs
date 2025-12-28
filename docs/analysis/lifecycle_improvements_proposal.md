# Propuestas de Mejora: Protección de Ciclos de Vida

**Fecha:** 2025-12-28  
**Versión:** 1.0  
**Estado:** Propuesta  
**Dependencias:** lifecycle_analysis_report.md

---

## Tabla de Contenidos

1. [Introducción](#1-introducción)
2. [Propuesta 1: Validación Centralizada WorkerState](#2-propuesta-1-validación-centralizada-workerstate)
3. [Propuesta 2: Job State Timeout Monitor](#3-propuesta-2-job-state-timeout-monitor)
4. [Propuesta 3: EventBus Resiliente](#4-propuesta-3-eventbus-resiliente)
5. [Propuesta 4: Propagación Automática de correlation_id](#5-propuesta-4-propagación-automática-de-correlation_id)
6. [Propuesta 5: Dead Letter Queue para Outbox](#6-propuesta-5-dead-letter-queue-para-outbox)
7. [Propuesta 6: Circuit Breaker para Worker Dispatch](#7-propuesta-6-circuit-breaker-para-worker-dispatch)
8. [Propuesta 7: Observabilidad Mejorada](#8-propuesta-7-observabilidad-mejorada)
9. [Plan de Implementación](#9-plan-de-implementación)

---

## 1. Introducción

Este documento presenta propuestas detalladas de implementación para proteger los ciclos de vida de Jobs y Workers, garantizando:

- **Consistencia:** Transiciones de estado validadas y atómicas
- **Resiliencia:** Recovery automático de fallos
- **Trazabilidad:** Correlación completa de flujos
- **Observabilidad:** Detección temprana de anomalías

---

## 2. Propuesta 1: Validación Centralizada WorkerState

### 2.1 Problema

WorkerState no tiene un método `can_transition_to()` como JobState, lo que lleva a validaciones ad-hoc en cada método del aggregate.

### 2.2 Solución Propuesta

#### Archivo: `crates/shared/src/states.rs`

```rust
impl WorkerState {
    /// Valida si una transición de estado es válida según el State Machine del Worker
    ///
    /// Transiciones válidas:
    /// - Creating → Connecting, Terminated (error durante creación)
    /// - Connecting → Ready, Terminated (error de conexión)
    /// - Ready → Busy, Draining, Terminating
    /// - Busy → Ready (job completado), Draining (graceful shutdown), Terminated (efímero)
    /// - Draining → Terminating, Terminated, Ready (drain cancelled)
    /// - Terminating → Terminated
    /// - Terminated → (estado terminal, no transiciones salientes)
    pub fn can_transition_to(&self, new_state: &WorkerState) -> bool {
        match (self, new_state) {
            // Mismo estado - no es válido
            (s, n) if s == n => false,

            // Desde Creating
            (WorkerState::Creating, WorkerState::Connecting) => true,
            (WorkerState::Creating, WorkerState::Terminated) => true, // Error durante creación

            // Desde Connecting
            (WorkerState::Connecting, WorkerState::Ready) => true,
            (WorkerState::Connecting, WorkerState::Terminated) => true, // Error de conexión

            // Desde Ready
            (WorkerState::Ready, WorkerState::Busy) => true,      // Job asignado
            (WorkerState::Ready, WorkerState::Draining) => true,  // Graceful shutdown
            (WorkerState::Ready, WorkerState::Terminating) => true, // Terminación directa

            // Desde Busy
            (WorkerState::Busy, WorkerState::Ready) => true,      // Job completado (non-ephemeral)
            (WorkerState::Busy, WorkerState::Draining) => true,   // Graceful shutdown solicitado
            (WorkerState::Busy, WorkerState::Terminated) => true, // Modelo efímero EPIC-21

            // Desde Draining
            (WorkerState::Draining, WorkerState::Ready) => true,       // Drain cancelado
            (WorkerState::Draining, WorkerState::Terminating) => true,
            (WorkerState::Draining, WorkerState::Terminated) => true,

            // Desde Terminating
            (WorkerState::Terminating, WorkerState::Terminated) => true,

            // Todas las demás transiciones son inválidas
            _ => false,
        }
    }

    /// Retorna true si el estado es terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkerState::Terminated)
    }

    /// Retorna true si el worker está en un estado que puede ejecutar trabajo
    pub fn is_operational(&self) -> bool {
        matches!(self, WorkerState::Ready | WorkerState::Busy)
    }

    /// Retorna la categoría del estado para reporting
    pub fn category(&self) -> &'static str {
        match self {
            WorkerState::Creating | WorkerState::Connecting => "provisioning",
            WorkerState::Ready => "available",
            WorkerState::Busy => "working",
            WorkerState::Draining | WorkerState::Terminating => "shutting_down",
            WorkerState::Terminated => "terminated",
        }
    }
}
```

#### Actualización del Worker Aggregate: `crates/server/domain/src/workers/aggregate.rs`

```rust
impl Worker {
    /// Transición genérica validada
    fn transition_to(&mut self, new_state: WorkerState, reason: &str) -> Result<WorkerState> {
        let old_state = self.state.clone();
        
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidWorkerStateTransition {
                worker_id: self.id.clone(),
                from_state: old_state.to_string(),
                to_state: new_state.to_string(),
                reason: reason.to_string(),
            });
        }
        
        self.state = new_state.clone();
        self.updated_at = Utc::now();
        
        Ok(old_state)
    }

    /// Worker conectando (refactorizado)
    pub fn mark_connecting(&mut self) -> Result<WorkerState> {
        self.transition_to(WorkerState::Connecting, "Agent starting gRPC connection")
    }

    /// Worker listo (refactorizado)
    pub fn mark_ready(&mut self) -> Result<WorkerState> {
        let old_state = self.transition_to(WorkerState::Ready, "Agent registered successfully")?;
        self.last_heartbeat = Utc::now();
        Ok(old_state)
    }

    /// Asignar job (refactorizado)
    pub fn assign_job(&mut self, job_id: JobId) -> Result<WorkerState> {
        let old_state = self.transition_to(WorkerState::Busy, "Job assigned")?;
        self.current_job_id = Some(job_id);
        Ok(old_state)
    }

    /// Job completado (refactorizado)
    pub fn complete_job(&mut self) -> Result<WorkerState> {
        let old_state = self.transition_to(
            WorkerState::Terminated,
            "Job completed - ephemeral worker terminating"
        )?;
        self.current_job_id = None;
        self.jobs_executed += 1;
        self.job_completed_at = Some(Utc::now());
        Ok(old_state)
    }
}
```

### 2.3 Tests Requeridos

```rust
#[cfg(test)]
mod worker_state_transitions {
    use super::*;

    #[test]
    fn test_valid_lifecycle_transitions() {
        // Happy path
        assert!(WorkerState::Creating.can_transition_to(&WorkerState::Connecting));
        assert!(WorkerState::Connecting.can_transition_to(&WorkerState::Ready));
        assert!(WorkerState::Ready.can_transition_to(&WorkerState::Busy));
        assert!(WorkerState::Busy.can_transition_to(&WorkerState::Terminated));
    }

    #[test]
    fn test_invalid_transitions() {
        // Cannot go backwards
        assert!(!WorkerState::Ready.can_transition_to(&WorkerState::Creating));
        assert!(!WorkerState::Busy.can_transition_to(&WorkerState::Connecting));
        
        // Cannot skip states
        assert!(!WorkerState::Creating.can_transition_to(&WorkerState::Busy));
        
        // Terminal state has no transitions
        assert!(!WorkerState::Terminated.can_transition_to(&WorkerState::Ready));
    }

    #[test]
    fn test_same_state_transition_invalid() {
        assert!(!WorkerState::Ready.can_transition_to(&WorkerState::Ready));
    }
}
```

---

## 3. Propuesta 2: Job State Timeout Monitor

### 3.1 Problema

Jobs pueden quedar atascados en estados intermedios (ASSIGNED, SCHEDULED) sin mecanismo de recovery.

### 3.2 Solución Propuesta

#### Nuevo Archivo: `crates/server/application/src/jobs/timeout_monitor.rs`

```rust
//! Job State Timeout Monitor
//!
//! Detecta y maneja jobs que han estado en estados intermedios
//! por más tiempo del permitido.

use chrono::{DateTime, Duration, Utc};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::{Job, JobRepository};
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobState, Result};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Configuration for timeout monitoring
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Time a job can be in ASSIGNED state before timeout (default: 5 min)
    pub assigned_timeout: Duration,
    /// Time a job can be in SCHEDULED state before timeout (default: 10 min)
    pub scheduled_timeout: Duration,
    /// Check interval (default: 30 sec)
    pub check_interval: std::time::Duration,
    /// Maximum jobs to process per check cycle
    pub batch_size: usize,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            assigned_timeout: Duration::minutes(5),
            scheduled_timeout: Duration::minutes(10),
            check_interval: std::time::Duration::from_secs(30),
            batch_size: 100,
        }
    }
}

/// Result of a timeout check cycle
#[derive(Debug, Default)]
pub struct TimeoutCheckResult {
    pub checked_count: usize,
    pub timed_out_assigned: Vec<JobId>,
    pub timed_out_scheduled: Vec<JobId>,
    pub recovered_count: usize,
    pub failed_count: usize,
}

/// Job State Timeout Monitor Service
pub struct JobStateTimeoutMonitor {
    job_repository: Arc<dyn JobRepository>,
    event_bus: Arc<dyn EventBus>,
    config: TimeoutConfig,
}

impl JobStateTimeoutMonitor {
    pub fn new(
        job_repository: Arc<dyn JobRepository>,
        event_bus: Arc<dyn EventBus>,
        config: TimeoutConfig,
    ) -> Self {
        Self {
            job_repository,
            event_bus,
            config,
        }
    }

    /// Start the background monitoring task
    pub fn start(&self) -> mpsc::Receiver<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        let monitor = self.clone_for_task();
        let interval = self.config.check_interval;
        
        tokio::spawn(async move {
            info!("🕐 JobStateTimeoutMonitor: Starting background monitor");
            
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                match monitor.run_check().await {
                    Ok(result) => {
                        if result.timed_out_assigned.len() + result.timed_out_scheduled.len() > 0 {
                            info!(
                                "🕐 TimeoutMonitor: {} ASSIGNED timeouts, {} SCHEDULED timeouts",
                                result.timed_out_assigned.len(),
                                result.timed_out_scheduled.len()
                            );
                        }
                    }
                    Err(e) => {
                        error!("❌ TimeoutMonitor check failed: {}", e);
                    }
                }
            }
        });
        
        shutdown_rx
    }

    fn clone_for_task(&self) -> Self {
        Self {
            job_repository: self.job_repository.clone(),
            event_bus: self.event_bus.clone(),
            config: self.config.clone(),
        }
    }

    /// Run a single timeout check cycle
    pub async fn run_check(&self) -> Result<TimeoutCheckResult> {
        let mut result = TimeoutCheckResult::default();
        let now = Utc::now();

        // Check ASSIGNED jobs
        let assigned_cutoff = now - self.config.assigned_timeout;
        let stale_assigned = self.find_stale_jobs(JobState::Assigned, assigned_cutoff).await?;
        
        for job in stale_assigned {
            result.checked_count += 1;
            match self.handle_timeout(&job, "assigned_timeout").await {
                Ok(()) => {
                    result.timed_out_assigned.push(job.id.clone());
                    result.recovered_count += 1;
                }
                Err(e) => {
                    error!("Failed to handle timeout for job {}: {}", job.id, e);
                    result.failed_count += 1;
                }
            }
        }

        // Check SCHEDULED jobs
        let scheduled_cutoff = now - self.config.scheduled_timeout;
        let stale_scheduled = self.find_stale_jobs(JobState::Scheduled, scheduled_cutoff).await?;
        
        for job in stale_scheduled {
            result.checked_count += 1;
            match self.handle_timeout(&job, "scheduled_timeout").await {
                Ok(()) => {
                    result.timed_out_scheduled.push(job.id.clone());
                    result.recovered_count += 1;
                }
                Err(e) => {
                    error!("Failed to handle timeout for job {}: {}", job.id, e);
                    result.failed_count += 1;
                }
            }
        }

        Ok(result)
    }

    async fn find_stale_jobs(&self, state: JobState, before: DateTime<Utc>) -> Result<Vec<Job>> {
        self.job_repository
            .find_by_state_older_than(&state, before, self.config.batch_size)
            .await
    }

    async fn handle_timeout(&self, job: &Job, reason: &str) -> Result<()> {
        let old_state = job.state().clone();
        let mut job = job.clone();
        
        // Transicionar a FAILED
        job.fail(format!("State timeout: {} exceeded for state {:?}", reason, old_state))?;
        
        // Persistir cambio
        self.job_repository.update(&job).await?;
        
        // Publicar evento
        let event = DomainEvent::JobStatusChanged {
            job_id: job.id.clone(),
            old_state: old_state.clone(),
            new_state: JobState::Failed,
            occurred_at: Utc::now(),
            correlation_id: Some(format!("timeout-{}-{}", reason, job.id.0)),
            actor: Some("system:timeout_monitor".to_string()),
        };
        
        self.event_bus.publish(&event).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to publish timeout event: {}", e),
            }
        })?;

        warn!(
            "⏰ Job {} timed out in {:?} state (reason: {})",
            job.id, old_state, reason
        );
        
        Ok(())
    }
}
```

#### Extensión requerida en JobRepository

```rust
// Agregar a crates/server/domain/src/jobs/repository.rs
#[async_trait]
pub trait JobRepository: Send + Sync {
    // ... métodos existentes ...

    /// Find jobs in a specific state that haven't been updated since `before`
    async fn find_by_state_older_than(
        &self,
        state: &JobState,
        before: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Job>>;
}
```

---

## 4. Propuesta 3: EventBus Resiliente

### 4.1 Problema

Si el stream de PostgreSQL LISTEN se desconecta, el JobCoordinator pierde eventos.

### 4.2 Solución Propuesta

#### Nuevo Archivo: `crates/server/infrastructure/src/messaging/resilient_subscriber.rs`

```rust
//! Resilient Event Subscriber
//!
//! Wrapper que proporciona reconexión automática para streams de eventos.

use futures::stream::BoxStream;
use futures::StreamExt;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Configuration for resilient subscription
#[derive(Debug, Clone)]
pub struct ResilientSubscriberConfig {
    /// Initial reconnection delay
    pub initial_delay: Duration,
    /// Maximum reconnection delay (cap for exponential backoff)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Maximum reconnection attempts before giving up (0 = infinite)
    pub max_attempts: usize,
}

impl Default for ResilientSubscriberConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: 0, // Infinite retries
        }
    }
}

/// Resilient subscriber that auto-reconnects on failures
pub struct ResilientSubscriber {
    event_bus: Arc<dyn EventBus>,
    channel: String,
    config: ResilientSubscriberConfig,
}

impl ResilientSubscriber {
    pub fn new(
        event_bus: Arc<dyn EventBus>,
        channel: impl Into<String>,
        config: ResilientSubscriberConfig,
    ) -> Self {
        Self {
            event_bus,
            channel: channel.into(),
            config,
        }
    }

    /// Start resilient subscription and return a channel receiver
    pub async fn subscribe(&self) -> mpsc::Receiver<Result<DomainEvent, EventBusError>> {
        let (tx, rx) = mpsc::channel(100);
        
        let event_bus = self.event_bus.clone();
        let channel = self.channel.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut attempt = 0;
            let mut delay = config.initial_delay;
            
            loop {
                info!("🔌 ResilientSubscriber: Connecting to channel '{}'", channel);
                
                match event_bus.subscribe(&channel).await {
                    Ok(mut stream) => {
                        info!("✅ ResilientSubscriber: Connected to '{}'", channel);
                        attempt = 0;
                        delay = config.initial_delay;
                        
                        // Process events until stream ends or errors
                        loop {
                            match stream.next().await {
                                Some(Ok(event)) => {
                                    if tx.send(Ok(event)).await.is_err() {
                                        info!("ResilientSubscriber: Receiver dropped, shutting down");
                                        return;
                                    }
                                }
                                Some(Err(e)) => {
                                    error!("❌ ResilientSubscriber: Stream error: {}", e);
                                    let _ = tx.send(Err(e)).await;
                                    break; // Reconnect
                                }
                                None => {
                                    warn!("⚠️ ResilientSubscriber: Stream ended");
                                    break; // Reconnect
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ ResilientSubscriber: Failed to subscribe: {}", e);
                    }
                }
                
                // Check max attempts
                attempt += 1;
                if config.max_attempts > 0 && attempt >= config.max_attempts {
                    error!("❌ ResilientSubscriber: Max attempts reached, giving up");
                    return;
                }
                
                // Exponential backoff
                warn!(
                    "⏳ ResilientSubscriber: Reconnecting in {:?} (attempt {})",
                    delay, attempt
                );
                tokio::time::sleep(delay).await;
                
                delay = Duration::from_secs_f64(
                    (delay.as_secs_f64() * config.backoff_multiplier).min(config.max_delay.as_secs_f64())
                );
            }
        });
        
        rx
    }
}
```

---

## 5. Propuesta 4: Propagación Automática de correlation_id

### 5.1 Problema

`correlation_id` no se genera automáticamente en el punto de entrada (gRPC) y no se propaga consistentemente.

### 5.2 Solución Propuesta

#### Nuevo Archivo: `crates/server/interface/src/grpc/correlation.rs`

```rust
//! Correlation ID Propagation
//!
//! Middleware y utilities para propagación automática de correlation_id.

use tonic::{Request, Status};
use uuid::Uuid;

/// Key for correlation ID in gRPC metadata
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";

/// Wrapper type for correlation ID
#[derive(Debug, Clone)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    /// Generate a new correlation ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from existing string
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the inner string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract or generate correlation ID from gRPC request
pub fn extract_correlation_id<T>(request: &Request<T>) -> CorrelationId {
    request
        .metadata()
        .get(CORRELATION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(CorrelationId::from_string)
        .unwrap_or_else(CorrelationId::new)
}

/// Request extension for correlation ID
pub trait CorrelationIdExt {
    fn correlation_id(&self) -> Option<CorrelationId>;
}

impl<T> CorrelationIdExt for Request<T> {
    fn correlation_id(&self) -> Option<CorrelationId> {
        self.extensions().get::<CorrelationId>().cloned()
    }
}
```

---

## 6. Propuesta 5: Dead Letter Queue para Outbox

### 6.1 Problema

Eventos del Outbox que fallan repetidamente no tienen mecanismo de manejo.

### 6.2 Solución Propuesta

#### Migración SQL

```sql
-- migrations/20241228_add_outbox_dlq.sql

-- Dead Letter Queue table
CREATE TABLE IF NOT EXISTS outbox_dlq (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    original_event_id UUID NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_type VARCHAR(50) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    payload JSONB NOT NULL,
    metadata JSONB,
    error_message TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    original_created_at TIMESTAMPTZ NOT NULL,
    moved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    resolution_notes TEXT
);

CREATE INDEX idx_outbox_dlq_moved_at ON outbox_dlq(moved_at);
CREATE INDEX idx_outbox_dlq_event_type ON outbox_dlq(event_type);
CREATE INDEX idx_outbox_dlq_resolved ON outbox_dlq(resolved_at) WHERE resolved_at IS NULL;

-- Add max_retries column to outbox if not exists
ALTER TABLE outbox_events 
ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 0,
ADD COLUMN IF NOT EXISTS last_error TEXT;
```

---

## 7. Propuesta 6: Circuit Breaker para Worker Dispatch

### 7.1 Problema

Dispatch fallido a un worker puede causar reintentos infinitos o bloqueo del sistema.

### 7.2 Solución Propuesta

```rust
// Nuevo archivo: crates/server/application/src/resilience/circuit_breaker.rs

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,     // Normal operation
    Open,       // Failing, reject requests
    HalfOpen,   // Testing if recovered
}

pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: AtomicUsize,
    success_count: AtomicUsize,
    last_failure: Arc<RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub success_threshold: usize,
    pub timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: AtomicUsize::new(0),
            success_count: AtomicUsize::new(0),
            last_failure: Arc::new(RwLock::new(None)),
            config,
        }
    }

    pub async fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        // Check if circuit allows request
        if !self.allow_request().await {
            return Err(CircuitBreakerError::CircuitOpen);
        }

        match f.await {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(e) => {
                self.record_failure().await;
                Err(CircuitBreakerError::Inner(e))
            }
        }
    }

    async fn allow_request(&self) -> bool {
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(last_failure) = *self.last_failure.read().await {
                    if last_failure.elapsed() >= self.config.timeout {
                        *self.state.write().await = CircuitState::HalfOpen;
                        self.success_count.store(0, Ordering::SeqCst);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    async fn record_success(&self) {
        let state = *self.state.read().await;
        
        match state {
            CircuitState::HalfOpen => {
                let count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= self.config.success_threshold {
                    *self.state.write().await = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                }
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    async fn record_failure(&self) {
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => {
                let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= self.config.failure_threshold {
                    *self.state.write().await = CircuitState::Open;
                    *self.last_failure.write().await = Some(Instant::now());
                }
            }
            CircuitState::HalfOpen => {
                *self.state.write().await = CircuitState::Open;
                *self.last_failure.write().await = Some(Instant::now());
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    CircuitOpen,
    Inner(E),
}
```

---

## 8. Propuesta 7: Observabilidad Mejorada

### 8.1 Métricas Propuestas

```rust
// Nuevo archivo: crates/server/infrastructure/src/metrics/lifecycle_metrics.rs

use prometheus::{Counter, Gauge, Histogram, Registry};

pub struct LifecycleMetrics {
    // Job metrics
    pub jobs_created_total: Counter,
    pub jobs_completed_total: Counter,
    pub jobs_failed_total: Counter,
    pub jobs_timed_out_total: Counter,
    pub job_state_duration: Histogram,
    pub jobs_in_state: Gauge,  // By state label
    
    // Worker metrics
    pub workers_created_total: Counter,
    pub workers_terminated_total: Counter,
    pub worker_state_duration: Histogram,
    pub workers_in_state: Gauge,  // By state label
    
    // Event metrics
    pub events_published_total: Counter,
    pub events_failed_total: Counter,
    pub event_publish_duration: Histogram,
    pub outbox_queue_size: Gauge,
    pub dlq_size: Gauge,
    
    // Circuit breaker metrics
    pub circuit_breaker_state: Gauge,  // By worker_id label
    pub dispatch_failures_total: Counter,
}
```

### 8.2 Alertas Propuestas

```yaml
# monitoring/alerts/lifecycle_alerts.yml
groups:
  - name: job_lifecycle_alerts
    rules:
      - alert: JobsStuckInAssigned
        expr: sum(jobs_in_state{state="ASSIGNED"}) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Jobs stuck in ASSIGNED state"

      - alert: HighJobFailureRate
        expr: rate(jobs_failed_total[5m]) / rate(jobs_created_total[5m]) > 0.1
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "High job failure rate"

      - alert: OutboxQueueBacklog
        expr: outbox_queue_size > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Outbox queue has too many pending events"

      - alert: DLQNotEmpty
        expr: dlq_size > 0
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "Dead Letter Queue has events"
```

---

## 9. Plan de Implementación

### 9.1 Fases de Implementación

| Fase | Propuesta | Esfuerzo | Prioridad | Dependencias |
|------|-----------|----------|-----------|--------------|
| 1 | WorkerState can_transition_to | 0.5 días | P0 | Ninguna |
| 1 | Job State Timeout Monitor | 2 días | P0 | JobRepository extensión |
| 2 | EventBus Resiliente | 1 día | P0 | Ninguna |
| 2 | correlation_id Propagation | 1.5 días | P1 | Ninguna |
| 3 | Dead Letter Queue | 2 días | P1 | Migración SQL |
| 3 | Circuit Breaker | 1.5 días | P2 | Ninguna |
| 4 | Observabilidad | 2 días | P1 | Prometheus setup |

### 9.2 Checklist de Implementación

#### Fase 1 (Crítico - Semana 1)

- [ ] Agregar `can_transition_to()` a WorkerState
- [ ] Actualizar Worker aggregate para usar validación centralizada
- [ ] Crear `JobStateTimeoutMonitor`
- [ ] Extender JobRepository con `find_by_state_older_than`
- [ ] Tests unitarios para transiciones
- [ ] Tests de integración para timeout monitor

#### Fase 2 (Alta Prioridad - Semana 2)

- [ ] Implementar `ResilientSubscriber`
- [ ] Actualizar JobCoordinator para usar ResilientSubscriber
- [ ] Crear CorrelationIdInterceptor
- [ ] Propagación de correlation_id en eventos

#### Fase 3 (Media Prioridad - Semana 3)

- [ ] Migración SQL para DLQ
- [ ] Actualizar OutboxPoller con soporte DLQ
- [ ] Implementar CircuitBreaker
- [ ] Integrar CircuitBreaker en dispatch

#### Fase 4 (Observabilidad - Semana 4)

- [ ] Métricas de lifecycle
- [ ] Dashboard Grafana
- [ ] Alertas Prometheus

### 9.3 Criterios de Éxito

| Métrica | Actual | Objetivo |
|---------|--------|----------|
| Jobs huérfanos detectados | 0% | 100% |
| Tiempo recuperación EventBus | ∞ (manual) | < 1 min |
| Trazabilidad e2e | ~70% | 100% |
| Cobertura observabilidad | Básica | Completa |

---

*Documento de propuestas de mejora. Para implementar, seguir el plan de fases y validar con tests antes de merge.*

