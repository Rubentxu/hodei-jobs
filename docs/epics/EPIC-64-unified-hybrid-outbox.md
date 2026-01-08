# EPIC-64: Unified Hybrid Outbox Architecture

> **Document Version:** 1.0.0  
> **Date:** 2026-01-08  
> **Based On:** 
> - EPIC-63 (Hybrid Command Outbox)
> - Existing `OutboxRelay` implementation in `messaging/outbox_relay/relay.rs`
> - Proposed LISTEN/NOTIFY + Polling pattern
> **Approach:** TDD (Test-Driven Development) + Strangler Pattern

---

## Resumen Ejecutivo

Esta épica unifica la arquitectura de **Command Outbox** y **Event Outbox** bajo un patrón híbrido común, reutilizando componentes compartidos y eliminando duplicación de código.

### Problema Actual

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    ARQUITECTURA ACTUAL (DUPLICADA)                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Command Outbox                Event Outbox                             │
│  ┌─────────────────┐          ┌─────────────────┐                       │
│  │ outbox.rs       │          │ relay.rs        │                       │
│  │ - OutboxBus     │          │ - OutboxRelay   │                       │
│  │ - Relay         │          │ - Metrics       │                       │
│  │ - Repository    │          │ - Retry logic   │                       │
│  └─────────────────┘          └─────────────────┘                       │
│         │                            │                                   │
│         │                            │                                   │
│         ▼                            ▼                                   │
│  ┌─────────────────────────────────────────────────────┐               │
│  │                   DUPLICACIÓN                       │               │
│  │  - BackoffConfig    - PgNotifyListener              │               │
│  │  - Retry logic      - Metrics collection            │               │
│  │  - Dead letter      - Polling loop                  │               │
│  └─────────────────────────────────────────────────────┘               │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Solución Propuesta

```
┌─────────────────────────────────────────────────────────────────────────┐
│                   ARQUITECTURA UNIFICADA                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│                    ┌─────────────────────────┐                          │
│                    │   Shared Outbox Core    │                          │
│                    │  ┌───────────────────┐  │                          │
│                    │  │ PgNotifyListener  │  │                          │
│                    │  │ BackoffConfig     │  │                          │
│                    │  │ ScheduledAt trait │  │                          │
│                    │  │ DLQ Repository    │  │                          │
│                    │  └───────────────────┘  │                          │
│                    └───────────┬─────────────┘                          │
│                                │                                         │
│            ┌───────────────────┴───────────────────┐                    │
│            ▼                                       ▼                    │
│   ┌─────────────────────┐             ┌─────────────────────┐          │
│   │  Command Outbox     │             │   Event Outbox      │          │
│   │  (EPIC-63)          │             │   (Migrated)        │          │
│   │                     │             │                     │          │
│   │  - CommandOutboxBus │             │  - HybridOutboxRelay│          │
│   │  - CommandOutboxRepo│             │  - EventPublisher   │          │
│   │  - CommandDispatcher│             │  - EventConverter   │          │
│   └─────────────────────┘             └─────────────────────┘          │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Beneficios de la Unificación

| Aspecto | Antes | Después |
|---------|-------|---------|
| **Líneas de código** | ~2,500 duplicadas | ~800 compartidas |
| **BackoffConfig** | Duplicado en ambos | Uno compartido |
| **PgNotifyListener** | Por implementar en Commands | Uno compartido |
| **Retry logic** | Diferente implementación | Estrategia unificada |
| **Mantenimiento** | Cambios en 2 lugares | Un solo lugar |

---

## Arquitectura de Componentes Compartidos

### 1. PgNotifyListener (Compartido)

```rust
// crates/server/infrastructure/src/messaging/pg_notify_listener.rs

use sqlx::postgres::{PgListener, PgNotification};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Wrapper para PostgreSQL LISTEN/NOTIFY
#[derive(Debug)]
pub struct PgNotifyListener {
    listener: PgListener,
    tx: mpsc::Sender<PgNotification>,
    // Mantener alive el receiver
    _rx: mpsc::Receiver<PgNotification>,
}

impl PgNotifyListener {
    /// Crear listener para un canal específico
    pub async fn new(pool: &sqlx::PgPool, channel: &str) -> Result<Self, sqlx::Error> {
        let mut listener = PgListener::connect_with(pool).await?;
        listener.listen(channel).await?;

        let (tx, rx) = mpsc::channel(100);
        
        // Spawn background task para forwarding
        let mut listener = listener;
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                match listener.recv().await {
                    Ok(notification) => {
                        if tx.send(notification).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("PgListener error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            listener,
            tx,
            _rx: rx,
        })
    }

    /// Enviar notificación (para testing)
    pub async fn notify(&self, payload: &str) -> Result<(), sqlx::Error> {
        self.listener.notify("outbox_work", Some(payload)).await
    }

    /// Canal de notificaciones
    pub fn channel(&self) -> &str {
        "outbox_work"
    }
}

/// Extensión para diferentes canales
impl PgNotifyListener {
    /// Listener para comandos
    pub async fn for_commands(pool: &sqlx::PgPool) -> Result<Self, sqlx::Error> {
        Self::new(pool, "outbox_work").await
    }

    /// Listener para eventos
    pub async fn for_events(pool: &sqlx::PgPool) -> Result<Self, sqlx::Error> {
        Self::new(pool, "event_work").await
    }
}
```

### 2. BackoffConfig (Compartido)

```rust
// crates/server/infrastructure/src/messaging/backoff.rs

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};

/// Configuración de exponential backoff reutilizable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    /// Delay base en segundos (default: 5)
    #[serde(default = "default_base_delay")]
    pub base_delay_secs: i64,

    /// Delay máximo en segundos (default: 1800 = 30min)
    #[serde(default = "default_max_delay")]
    pub max_delay_secs: i64,

    /// Factor de jitter (0.0-1.0, default: 0.1 = ±10%)
    #[serde(default = "default_jitter")]
    pub jitter_factor: f64,

    /// Máximo reintentos antes de dead letter
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,
}

fn default_base_delay() -> i64 { 5 }
fn default_max_delay() -> i64 { 1800 }
fn default_jitter() -> f64 { 0.1 }
fn default_max_retries() -> i32 { 5 }

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base_delay_secs: default_base_delay(),
            max_delay_secs: default_max_delay(),
            jitter_factor: default_jitter(),
            max_retries: default_max_retries(),
        }
    }
}

impl BackoffConfig {
    /// Crear configuración estándar
    pub fn standard() -> Self {
        Self::default()
    }

    /// Crear configuración agresiva (para eventos críticos)
    pub fn aggressive() -> Self {
        Self {
            base_delay_secs: 1,
            max_delay_secs: 300,
            jitter_factor: 0.2,
            max_retries: 3,
        }
    }

    /// Calcular delay para un retry específico
    pub fn calculate_delay(&self, retry_count: i32) -> Duration {
        // Exponential: base * 2^retry_count
        let raw_delay = self.base_delay_secs * 2i64.pow(retry_count as u32);
        let delay = raw_delay.min(self.max_delay_secs);

        // Apply jitter
        let jitter_range = (delay as f64 * self.jitter_factor) as i64;
        let jitter = if jitter_range > 0 {
            let mut rng = rand::thread_rng();
            let jitter_value = rng.gen_range(-jitter_range..jitter_range);
            jitter_value
        } else {
            0
        };

        Duration::seconds(delay + jitter)
    }

    /// Calcular timestamp de próximo retry
    pub fn next_retry_at(&self, retry_count: i32) -> DateTime<Utc> {
        Utc::now() + self.calculate_delay(retry_count)
    }

    /// Verificar si se puede reintentar
    pub fn can_retry(&self, retry_count: i32) -> bool {
        retry_count < self.max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    fn test_backoff_calculation() {
        let config = BackoffConfig::standard();

        // Retry 0: ~5 segundos (±10%)
        let delay0 = config.calculate_delay(0).num_seconds();
        assert!(delay0 >= 4 && delay0 <= 6, "Retry 0: expected ~5s, got {}", delay0);

        // Retry 1: ~10 segundos
        let delay1 = config.calculate_delay(1).num_seconds();
        assert!(delay1 >= 8 && delay1 <= 12, "Retry 1: expected ~10s, got {}", delay1);

        // Retry 2: ~20 segundos
        let delay2 = config.calculate_delay(2).num_seconds();
        assert!(delay2 >= 16 && delay2 <= 24, "Retry 2: expected ~20s, got {}", delay2);
    }

    #[tokio::test]
    fn test_can_retry() {
        let config = BackoffConfig::default();
        assert!(config.can_retry(0));
        assert!(config.can_retry(4));
        assert!(!config.can_retry(5));
        assert!(!config.can_retry(10));
    }
}
```

### 3. ScheduledAt Trait (Compartido)

```rust
// crates/server/domain/src/outbox/scheduled_trait.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Trait para entidades que soportan scheduling
pub trait ScheduledAt {
    fn scheduled_at(&self) -> Option<DateTime<Utc>>;
    fn set_scheduled_at(&mut self, scheduled_at: DateTime<Utc>);
    fn is_ready(&self) -> bool {
        self.scheduled_at()
            .map(|at| Utc::now() >= at)
            .unwrap_or(true)
    }
}

/// Extensión para scheduled status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduledStatus {
    Pending,      // No programado
    Scheduled,    // Programado para el futuro
    Ready,        // Listo para procesar
    Processing,   // Actualmente procesando
    Completed,    // Completado
    Failed,       // Falló (reintentará)
    DeadLetter,   // En cola de muertos
}

impl Default for ScheduledStatus {
    fn default() -> Self {
        ScheduledStatus::Pending
    }
}
```

---

## Tabla de Historias de Usuario

| ID | Historia | Estado | Prioridad | Dependencias |
|----|----------|--------|-----------|--------------|
| 64.1 | Extract PgNotifyListener to shared module | ⏳ | P0 | - |
| 64.2 | Extract BackoffConfig to shared module | ⏳ | P0 | 64.1 |
| 64.3 | Add scheduled_at column to events table | ⏳ | P0 | - |
| 64.4 | Create unified ScheduledAt trait | ⏳ | P1 | 64.1, 64.2 |
| 64.5 | Migrate Event OutboxRelay to hybrid | ⏳ | P0 | 64.1, 64.3 |
| 64.6 | Migrate Command OutboxRelay to hybrid | ⏳ | P0 | EPIC-63, 64.1 |
| 64.7 | Add DLQ methods to Event Repository | ⏳ | P1 | 64.4 |
| 64.8 | Integration tests for unified architecture | ⏳ | P1 | 64.5, 64.6 |

---

## Historia de Usuario 64.1: Extraer PgNotifyListener

**Como** arquitecto de infraestructura  
**Quiero** un módulo compartido para PgNotifyListener  
**Para** reutilizarlo en Command y Event outboxes

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que verifica PgNotifyListener se puede usar para comandos
- [ ] **GREEN:** Implementar `PgNotifyListener` en módulo compartido
- [ ] **REFACTOR:** Mover lógica de testing de `outbox.rs` a shared

### Tests TDD (Red phase):

```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_pg_notify_listener_shared_module() {
    // Arrange
    let pool = setup_test_db().await;
    
    // Act - Usar PgNotifyListener desde módulo compartido
    let listener = PgNotifyListener::for_commands(&pool).await.unwrap();
    
    // Assert
    assert!(listener.channel() == "outbox_work");
    
    let listener_events = PgNotifyListener::for_events(&pool).await.unwrap();
    assert!(listener_events.channel() == "event_work");
}
```

### Ubicación del código:

```
crates/server/infrastructure/src/messaging/
├── pg_notify_listener.rs      (NUEVO - compartido)
├── outbox_relay/
│   ├── relay.rs               (usará shared)
│   └── mod.rs
└── nats_outbox_relay.rs       (usará shared para backoff)
```

---

## Historia de Usuario 64.2: Extraer BackoffConfig

**Como** desarrollador  
**Quiero** BackoffConfig en un módulo compartido  
**Para** no duplicar la lógica de retry en ambos outboxes

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que BackoffConfig funciona igual en ambos contextos
- [ ] **GREEN:** Mover `BackoffConfig` de `OutboxRelayConfig` a shared
- [ ] **REFACTOR:** Actualizar `OutboxRelayConfig` para usar shared

### Tests TDD (Red phase):

```rust
#[tokio::test]
fn test_backoff_config_shared_reuse() {
    // Arrange
    let shared_config = BackoffConfig::standard();
    
    // Act - Usar en contexto de comandos
    let command_delay = shared_config.calculate_delay(2);
    
    // Assert
    assert_eq!(command_delay.num_seconds(), 20);
    
    // Usar en contexto de eventos (mismo resultado)
    let event_delay = shared_config.calculate_delay(2);
    assert_eq!(command_delay, event_delay);
}

#[tokio::test]
fn test_backoff_config_presets() {
    // Standard
    let standard = BackoffConfig::standard();
    assert_eq!(standard.base_delay_secs, 5);
    assert_eq!(standard.max_retries, 5);
    
    // Aggressive
    let aggressive = BackoffConfig::aggressive();
    assert_eq!(aggressive.base_delay_secs, 1);
    assert_eq!(aggressive.max_retries, 3);
}
```

### Migración del código existente:

```rust
// ANTES: En relay.rs
pub struct OutboxRelayConfig {
    pub retry_delay: Duration,        // Duplicado
    pub max_retry_delay: Duration,    // Duplicado
}

// DESPUÉS: En shared/backoff.rs
pub struct BackoffConfig {
    pub base_delay_secs: i64,
    pub max_delay_secs: i64,
}

// En relay.rs
pub struct OutboxRelayConfig {
    pub backoff: BackoffConfig,       // Reutilizado
    pub batch_size: usize,
    pub poll_interval: Duration,
}
```

---

## Historia de Usuario 64.3: Añadir scheduled_at a tabla de eventos

**Como** sistema de eventos  
**Quiero** que los eventos puedan programarse para procesamiento futuro  
**Para** implementar retry con exponential backoff

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que verifica columna `scheduled_at` existe
- [ ] **GREEN:** Migration SQL añade columna a `hodei_events`
- [ ] **REFACTOR:** Actualizar `PostgresOutboxRepository` para usar `scheduled_at`

### Migration SQL:

```sql
-- Archivo: migrations/YYYYMMDDHHMMSS_add_scheduled_at_to_events/up.sql

-- Añadir columna scheduled_at si no existe
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'hodei_events' AND column_name = 'scheduled_at'
    ) THEN
        ALTER TABLE hodei_events
        ADD COLUMN scheduled_at TIMESTAMPTZ DEFAULT NOW();
    END IF;
    
    -- Añadir error_details para DLQ
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'hodei_events' AND column_name = 'error_details'
    ) THEN
        ALTER TABLE hodei_events
        ADD COLUMN error_details JSONB;
    END IF;
END $$;

-- Actualizar constraint de status para incluir RETRY
ALTER TABLE hodei_events DROP CONSTRAINT IF EXISTS hodei_events_status_check;
ALTER TABLE hodei_events ADD CONSTRAINT hodei_events_status_check
    CHECK (status IN ('PENDING', 'RETRY', 'COMPLETED', 'FAILED', 'DEAD_LETTER', 'CANCELLED'));

-- Trigger de notificación para eventos
CREATE OR REPLACE FUNCTION notify_event_insertion()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('event_work', NEW.id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_event_notify ON hodei_events;
CREATE TRIGGER trg_event_notify
AFTER INSERT ON hodei_events
FOR EACH ROW
EXECUTE FUNCTION notify_event_insertion();
```

### Tests TDD (Red phase):

```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_events_table_has_scheduled_at() {
    let pool = setup_test_db().await;
    let repo = PostgresOutboxRepository::new(pool);
    
    // Insertar evento
    let event = make_test_event();
    repo.insert_events(&[event]).await.unwrap();
    
    // Verificar scheduled_at
    let pending = repo.get_pending_events(10, 5).await.unwrap();
    assert!(!pending.is_empty());
    assert!(pending[0].scheduled_at.is_some());
}
```

---

## Historia de Usuario 64.4: Crear ScheduledAt Trait Unificado

**Como** desarrollador  
**Quiero** un trait `ScheduledAt` que funcione para comandos y eventos  
**Para** abstraer la lógica de scheduling

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que ScheduledAt funciona para CommandOutboxRecord
- [ ] **GREEN:** Implementar trait en shared module
- [ ] **REFACTOR:** Actualizar repositorios para implementar trait

### Tests TDD (Red phase):

```rust
#[tokio::test]
fn test_scheduled_trait_for_commands() {
    let mut cmd = CommandOutboxRecord::default();
    
    // Initially ready (no scheduled_at)
    assert!(cmd.is_ready());
    
    // Schedule for future
    let future = Utc::now() + Duration::hours(1);
    cmd.set_scheduled_at(future);
    assert!(!cmd.is_ready());
    
    // Schedule for past
    let past = Utc::now() - Duration::hours(1);
    cmd.set_scheduled_at(past);
    assert!(cmd.is_ready());
}

#[tokio::test]
fn test_scheduled_trait_for_events() {
    let mut event = OutboxEventView::default();
    
    assert!(event.is_ready());
    
    let future = Utc::now() + Duration::minutes(30);
    event.set_scheduled_at(future);
    assert!(!event.is_ready());
}
```

---

## Historia de Usuario 64.5: Migrar Event OutboxRelay a Híbrido

**Como** sistema de eventos  
**Quiero** que el OutboxRelay use LISTEN/NOTIFY + polling  
**Para** obtener reactividad inmediata con fallback confiable

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que relay procesa evento tras notificación
- [ ] **GREEN:** Implementar `HybridOutboxRelay` genérico
- [ ] **REFACTOR:** Reemplazar `OutboxRelay` existente

### Tests TDD (Red phase):

```rust
/// Hybrid Outbox Relay - Reutilizable para comandos y eventos
#[derive(Debug, Clone)]
pub struct HybridOutboxRelay<Processor> {
    repository: Arc<dyn OutboxRepository>,
    event_bus: Arc<dyn EventBus>,
    listener: Arc<tokio::sync::Mutex<PgNotifyListener>>,
    config: HybridOutboxConfig,
    processor: Arc<Processor>,  // Tipo genérico para procesar
    shutdown: tokio::sync::broadcast::Sender<()>,
}

#[derive(Debug, Clone)]
pub struct HybridOutboxConfig {
    pub batch_size: usize,
    pub poll_interval: Duration,
    pub backoff: BackoffConfig,
}

impl Default for HybridOutboxConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            poll_interval: Duration::from_millis(500),
            backoff: BackoffConfig::standard(),
        }
    }
}

impl<Processor> HybridOutboxRelay<Processor>
where
    Processor: EventProcessor + Send + Sync + 'static,
{
    pub async fn new(
        pool: &sqlx::PgPool,
        event_bus: Arc<dyn EventBus>,
        processor: Arc<Processor>,
        config: Option<HybridOutboxConfig>,
    ) -> Result<Self, sqlx::Error> {
        let listener = Arc::new(tokio::sync::Mutex::new(
            PgNotifyListener::for_events(pool).await?
        ));
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        
        Ok(Self {
            repository: Arc::new(PostgresOutboxRepository::new(pool.clone())),
            event_bus,
            listener,
            config: config.unwrap_or_default(),
            processor,
            shutdown,
        })
    }

    /// Run el relay híbrido
    pub async fn run(&self) {
        info!("Starting hybrid event outbox relay");
        
        let mut interval = tokio::time::interval(self.config.poll_interval);
        let mut shutdown_rx = self.shutdown.subscribe();
        let mut notification_rx = {
            let mut listener = self.listener.lock().await;
            listener.recv()
        };

        loop {
            tokio::select! {
                // Canal 1: Notificación de PostgreSQL
                notification = &mut notification_rx => {
                    match notification {
                        Some(_) => {
                            trace!("Event notification received");
                            self.process_pending().await;
                        }
                        None => {
                            // Reconectar
                            tracing::warn!("Event listener disconnected");
                            let mut l = self.listener.lock().await;
                            *l = PgNotifyListener::for_events(&self.repository.pool())
                                .await
                                .ok()
                                .unwrap_or_else(|| panic!("Failed to reconnect event listener"));
                            notification_rx = l.recv();
                        }
                    }
                    notification_rx = self.listener.lock().await.recv();
                }
                // Canal 2: Polling de seguridad
                _ = interval.tick() => {
                    trace!("Safety polling tick");
                    self.process_pending().await;
                }
                // Canal 3: Shutdown
                _ = shutdown_rx.recv() => {
                    info!("Hybrid event relay shutting down");
                    break;
                }
            }
        }
    }

    async fn process_pending(&self) {
        if let Err(e) = self.repository.process_pending_batch(
            self.config.batch_size,
            &self.config.backoff,
        ).await {
            error!("Error processing event batch: {}", e);
        }
    }
}

#[tokio::test]
async fn test_hybrid_relay_processes_notification() {
    // Arrange
    let pool = setup_test_db().await;
    let event_bus = Arc::new(MockEventBus::default());
    let processor = Arc::new(MockEventProcessor::default());
    
    let relay = HybridOutboxRelay::new(&pool, event_bus, processor, None)
        .await
        .unwrap();
    
    // Act - Insertar evento (disparará pg_notify)
    insert_test_event(&pool).await;
    
    // Simular procesamiento
    relay.process_pending().await;
    
    // Assert - Verificar procesamiento
    // ...
}
```

---

## Historia de Usuario 64.6: Migrar Command OutboxRelay a Híbrido

**Como** sistema de comandos  
**Quiero** que el CommandOutboxRelay use la misma arquitectura híbrida  
**Para** mantener consistencia con eventos

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que CommandRelay y EventRelay comparten componentes
- [ ] **GREEN:** Implementar `CommandProcessor` trait
- [ ] **REFACTOR:** Unificar con `HybridOutboxRelay`

### Tests TDD (Red phase):

```rust
/// Command Processor trait para HybridOutboxRelay
#[async_trait::async_trait]
pub trait CommandProcessor: Send + Sync {
    async fn process_command(&self, command: &CommandOutboxRecord) 
        -> Result<(), CommandOutboxError>;
    
    async fn process_batch(&self, commands: &[CommandOutboxRecord]) 
        -> Vec<Result<(), CommandOutboxError>> {
        let mut results = Vec::new();
        for cmd in commands {
            results.push(self.process_command(cmd).await);
        }
        results
    }
}

/// Implementación concreta para comandos
#[derive(Clone)]
pub struct CommandProcessorImpl {
    command_bus: DynCommandBus,
}

#[async_trait::async_trait]
impl CommandProcessor for CommandProcessorImpl {
    async fn process_command(&self, command: &CommandOutboxRecord) 
        -> Result<(), CommandOutboxError> {
        // Dispatch comando al CommandBus
        dispatch_erased(&self.command_bus, command).await?;
        Ok(())
    }
}
```

---

## Historia de Usuario 64.7: Añadir métodos DLQ a Event Repository

**Como** sistema de eventos  
**Quiero** métodos de Dead Letter Queue en el repositorio de eventos  
**Para** mover eventos fallidos a cola de muertos

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que verifica métodos DLQ en Event Repository
- [ ] **GREEN:** Implementar `mark_dead_letter()`, `get_dead_letters()`, etc.
- [ ] **REFACTOR:** Unificar con Command Repository DLQ

### Tests TDD (Red phase):

```rust
#[async_trait::async_trait]
pub trait DeadLetterQueue: Send + Sync {
    /// Mover entidad a dead letter
    async fn mark_dead_letter(
        &self,
        id: &Uuid,
        error: &str,
    ) -> Result<(), OutboxError>;

    /// Obtener todos los dead letters
    async fn get_dead_letters(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboxEventView>, OutboxError>;

    /// Reprocesar un dead letter
    async fn retry_dead_letter(&self, id: &Uuid) -> Result<(), OutboxError>;

    /// Obtener estadísticas de dead letters
    async fn get_dead_letter_stats(&self) -> Result<DeadLetterStats, OutboxError>;
}

#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_event_dead_letter_queue() {
    let pool = setup_test_db().await;
    let repo = PostgresOutboxRepository::new(pool);
    
    // Insertar evento
    let event_id = insert_test_event(&pool).await;
    
    // Simular múltiples fallos
    for i in 0..6 {
        repo.mark_failed(&event_id, &format!("Error {}", i)).await.unwrap();
    }
    
    // Verificar dead letter
    let dlq = repo.get_dead_letters(10).await.unwrap();
    assert!(dlq.iter().any(|e| e.id == event_id));
}
```

---

## Historia de Usuario 64.8: Integration Tests

**Como** sistema de integración  
**Quiero** tests que validen la arquitectura unificada  
**Para** garantizar que comandos y eventos funcionan igual

### Tests de Integración:

```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_unified_backoff_behavior() {
    // Arrange
    let config = BackoffConfig::standard();
    
    // Act - Calcular delays
    let delays: Vec<i64> = (0..5)
        .map(|i| config.calculate_delay(i).num_seconds())
        .collect();
    
    // Assert - Verificar exponential backoff
    assert_eq!(delays[0], 5);   // ~5s
    assert_eq!(delays[1], 10);  // ~10s
    assert_eq!(delays[2], 20);  // ~20s
    assert_eq!(delays[3], 40);  // ~40s
    assert_eq!(delays[4], 80);  // ~80s
}

#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_hybrid_relay_notifications() {
    let pool = setup_test_db().await;
    
    // Test Command Listener
    let cmd_listener = PgNotifyListener::for_commands(&pool).await.unwrap();
    assert_eq!(cmd_listener.channel(), "outbox_work");
    
    // Test Event Listener
    let event_listener = PgNotifyListener::for_events(&pool).await.unwrap();
    assert_eq!(event_listener.channel(), "event_work");
}

#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_both_outboxes_use_same_backoff() {
    let config = BackoffConfig::aggressive();
    
    // Commands
    let cmd_delay = config.calculate_delay(1);
    assert_eq!(cmd_delay.num_seconds(), 2);  // 1s * 2
    
    // Events
    let event_delay = config.calculate_delay(1);
    assert_eq!(event_delay.num_seconds(), 2);  // 1s * 2
    
    // Mismo resultado
    assert_eq!(cmd_delay, event_delay);
}
```

---

## Resumen de Archivos Afectados

```
crates/server/infrastructure/src/messaging/
├── mod.rs                           (MODIFICAR: exportar shared)
├── pg_notify_listener.rs            (NUEVO: compartido)
├── backoff.rs                       (NUEVO: compartido)
├── outbox_relay/
│   ├── mod.rs                       (MODIFICAR: usar shared)
│   └── relay.rs                     (REFACTOR: migrar a híbrido)
└── nats_outbox_relay.rs             (MODIFICAR: usar shared)

crates/server/domain/src/outbox/
├── mod.rs                           (MODIFICAR: exportar trait)
├── scheduled_trait.rs               (NUEVO: ScheduledAt trait)
└── postgres/
    ├── mod.rs                       (MODIFICAR: añadir DLQ)
    └── repository.rs                (MODIFICAR: usar scheduled_at)

crates/server/infrastructure/migrations/
└── YYYYMMDDHHMMSS_add_scheduled_at_to_events/
    ├── up.sql                       (NUEVO)
    └── down.sql                     (NUEVO)
```

---

## Plan de Migración (Strangler Pattern)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    MIGRACIÓN GRADUAL                                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Fase 1: Shared Components                                             │
│  ├── 64.1: PgNotifyListener compartido                                 │
│  └── 64.2: BackoffConfig compartido                                    │
│                                                                         │
│  Fase 2: Database Migration                                            │
│  └── 64.3: Añadir scheduled_at a eventos                               │
│                                                                         │
│  Fase 3: Event Outbox Migration                                        │
│  ├── 64.4: ScheduledAt trait                                           │
│  └── 64.5: HybridOutboxRelay para eventos                              │
│                                                                         │
│  Fase 4: Command Outbox Migration                                      │
│  └── 64.6: HybridOutboxRelay para comandos                             │
│                                                                         │
│  Fase 5: DLQ Unification                                               │
│  └── 64.7: Dead Letter Queue compartido                                │
│                                                                         │
│  Fase 6: Testing                                                       │
│  └── 64.8: Integration tests                                           │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Rolling Update Strategy

1. **Durante migración**: Ambos relés funcionan (old + new)
2. **Testing**: Verificar que new produce mismos resultados
3. **Cutover**: Switch gradual al new relay
4. **Cleanup**: Eliminar código duplicado old

---

## Métricas de Éxito

| Métrica | Antes | Después | Objetivo |
|---------|-------|---------|----------|
| Líneas de código duplicadas | ~2,500 | ~800 | -68% |
| Latencia de notificación (eventos) | 500ms polling | <100ms | -80% |
| Reutilización de BackoffConfig | 0% | 100% | ✅ |
| Cobertura de tests | 70% | 90% | +20% |

---

## Dependencias

```
EPIC-64 (Unified Hybrid Outbox)
    │
    ├── Depende de:
    │   ├── EPIC-63 (Command Outbox) ✅
    │   └── OutboxRelay existente ✅
    │
    └── Desbloquea:
        └── EPIC-51 (Transactional Outbox Pattern) - Completará
```

---

## Notas de Implementación

### Compatibilidad hacia atrás

- Mantener `OutboxRelayConfig` existente como alias
- `BackoffConfig` debe ser serializable igual que antes
- Los canales de notificación deben ser configurables

### Configuración de Producción

```yaml
# shared/backoff.yaml
backoff:
  # Común para ambos outboxes
  base_delay_secs: 5
  max_delay_secs: 1800  # 30 minutos
  jitter_factor: 0.1    # ±10%
  max_retries: 5

# events/relay.yaml
events:
  outbox:
    batch_size: 50
    poll_interval_ms: 500
    channel: "event_work"

# commands/relay.yaml
commands:
  outbox:
    batch_size: 10
    poll_interval_ms: 30000  # Más lento para comandos
    channel: "outbox_work"
```

### Monitoreo Unificado

```rust
// Métricas compartidas
pub struct OutboxMetrics {
    pub events_published_total: u64,
    pub commands_dispatched_total: u64,
    pub dead_letter_total: u64,
    pub notifications_received: u64,
    pub polling_wakeups: u64,
}
```
