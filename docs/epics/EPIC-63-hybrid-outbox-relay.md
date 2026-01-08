# Épica 63: Hybrid Outbox Relay (LISTEN/NOTIFY + Polling)

> **Document Version:** 1.0.0  
> **Date:** 2026-01-08  
> **Based On:** Hybrid LISTEN/NOTIFY + Polling Pattern Proposal  
> **Approach:** TDD (Test-Driven Development)

---

## Resumen Ejecutivo

Esta épica implementa el **patrón híbrido LISTEN/NOTIFY + polling** para el Command Outbox, considerado el "estándar de oro" para sistemas PostgreSQL. Combina la reactividad de notificaciones de base de datos con la seguridad de polling periódico.

### Beneficios Clave

| Aspecto | Antes (Solo Polling) | Después (Híbrido) |
|---------|---------------------|-------------------|
| **Latencia** | Intervalo de polling (ej: 30s) | Inmediato (notificación push) |
| **Resiliencia** | Confía en polling | Doble garantía (notify + fallback) |
| **Eficiencia CPU** | polling constante | ~0% CPU en reposo |
| **Recuperación** | Manual | Automática con backoff exponencial |

---

## Arquitectura del Relay Híbrido

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           ARQUITECTURA HÍBRIDA                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─────────────┐     ┌──────────────────────┐                                 │
│   │    Saga     │     │  PostgreSQL          │                                 │
│   │             │     │  ┌────────────────┐  │                                 │
│   │  INSERT     │────▶│  │ hodei_commands │  │                                 │
│   │  Command    │     │  └────────────────┘  │                                 │
│   └─────────────┘     │         ▲            │                                 │
│                       │         │ TRIGGER    │                                 │
│                       │         ▼            │                                 │
│                       │  ┌────────────────┐  │                                 │
│                       │  │ pg_notify()    │──┼──┐                             │
│                       │  │ 'outbox_work'  │  │  │                             │
│                       │  └────────────────┘  │  │                             │
│                       └──────────────────────┘  │                             │
│                                                  │                             │
│   ┌──────────────────────────────────────────────┼─────────────────────────┐   │
│   │              Rust Relay Worker               │                         │   │
│   │                                              ▼                         │   │
│   │   ┌─────────────────────────────────────────────────────────────┐     │   │
│   │   │                    tokio::select!                            │     │   │
│   │   │  ┌──────────────────┐    ┌────────────────────────────┐     │     │   │
│   │   │  │ PgListener.recv()│    │ tokio::time::sleep(30s)   │     │     │   │
│   │   │  │ (NOTIFY reactivo)│    │ (Polling de seguridad)    │     │     │   │
│   │   │  └────────┬─────────┘    └─────────────┬────────────┘     │     │     │
│   │   │           │                             │                  │     │     │
│   │   │           ▼                             ▼                  │     │     │
│   │   │   ┌──────────────────────────────────────────────┐       │     │     │
│   │   │   │        process_pending_commands()           │       │     │     │
│   │   │   │   FOR UPDATE SKIP LOCKED LIMIT 10           │       │     │     │
│   │   │   └──────────────────────────────────────────────┘       │     │     │
│   │   │                       │                                 │     │     │
│   │   │                       ▼                                 │     │     │
│   │   │   ┌──────────────────────────────────────────────┐       │     │     │
│   │   │   │           CommandBus.dispatch_erased()       │       │     │     │
│   │   │   └──────────────────────────────────────────────┘       │     │     │
│   │   │                       │                                 │     │     │
│   │   │           ┌───────────┴───────────┐                    │     │     │
│   │   │           ▼                       ▼                    │     │     │
│   │   │   ┌──────────────┐      ┌──────────────────────┐      │     │     │
│   │   │   │  COMPLETED   │      │ FAILED → Retry/DEAD  │      │     │     │
│   │   │   └──────────────┘      └──────────────────────┘      │     │     │
│   │   └────────────────────────────────────────────────────────┘     │     │
│   └──────────────────────────────────────────────────────────────────┘     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Componentes a Implementar

1. **Trigger SQL**: PostgreSQL NOTIFY al INSERT
2. **PgListener**: Rust LISTEN para notificaciones
3. **HybridRelay**: tokio::select! con ambas fuentes
4. **Backoff Engine**: Exponential backoff + jitter
5. **Dead Letter Queue**: Cola para intervención manual
6. **Saga Reaper**: Limpieza de comandos huérfanos

---

## Esquema de Base de Datos

### Migración: Añadir columnas de scheduling

```sql
-- En crates/server/infrastructure/migrations/20260108_add_hodei_commands_table/up.sql

-- Añadir columnas para scheduling y retry (si no existen)
ALTER TABLE hodei_commands ADD COLUMN IF NOT EXISTS scheduled_at TIMESTAMPTZ DEFAULT NOW();
ALTER TABLE hodei_commands ADD COLUMN IF NOT EXISTS max_retries INTEGER DEFAULT 5;
ALTER TABLE hodei_commands ADD COLUMN IF NOT EXISTS error_details JSONB;

-- Renombrar status para incluir RETRY
ALTER TABLE hodei_commands DROP CONSTRAINT IF EXISTS hodei_commands_status_check;
ALTER TABLE hodei_commands ADD CONSTRAINT hodei_commands_status_check
    CHECK (status IN ('PENDING', 'RETRY', 'COMPLETED', 'FAILED', 'DEAD_LETTER', 'CANCELLED'));

-- Trigger para NOTIFY
CREATE OR REPLACE FUNCTION notify_outbox_insertion()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('outbox_work', NEW.id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Eliminar trigger anterior si existe, crear nuevo
DROP TRIGGER IF EXISTS trg_outbox_notify ON hodei_commands;
CREATE TRIGGER trg_outbox_notify
AFTER INSERT ON hodei_commands
FOR EACH ROW
EXECUTE FUNCTION notify_outbox_insertion();

-- Índice para scheduled_at (queries eficientes de retry)
CREATE INDEX IF NOT EXISTS idx_hodei_commands_scheduled
ON hodei_commands(status, scheduled_at)
WHERE status IN ('PENDING', 'RETRY');
```

---

## Estados del Command Outbox

```rust
// En command/outbox.rs

/// Status de un comando en el outbox (AMPLIADO)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOutboxStatus {
    /// Comando creado, pendiente de procesamiento
    Pending,
    /// Comando programado para reintento
    Retry,
    /// Comando procesado exitosamente
    Completed,
    /// Comando falló permanentemente (dead letter)
    DeadLetter,
    /// Comando cancelado explícitamente
    Cancelled,
}

impl std::fmt::Display for CommandOutboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandOutboxStatus::Pending => write!(f, "PENDING"),
            CommandOutboxStatus::Retry => write!(f, "RETRY"),
            CommandOutboxStatus::Completed => write!(f, "COMPLETED"),
            CommandOutboxStatus::DeadLetter => write!(f, "DEAD_LETTER"),
            CommandOutboxStatus::Cancelled => write!(f, "CANCELLED"),
        }
    }
}
```

---

## Retry Strategy: Exponential Backoff

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        EXPONENTIAL BACKOFF STRATEGY                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Intento    Tiempo de Espera        Estrategia                              │
│  ────────   ──────────────────      ──────────                              │
│  1          5 segundos              Initial delay (configurable)            │
│  2          30 segundos             2^2 * 5                                 │
│  3          5 minutos               2^3 * 5                                 │
│  4          30 minutos              2^4 * 5                                 │
│  5          DEAD_LETTER             Límite alcanzado → Requiere干预         │
│                                                                             │
│  FÓRMULA: delay_ms = min(base_delay_ms * 2^retry_count, max_delay_ms)      │
│  + JITTER: ±10% para evitar thundering herd                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Tabla de Historias de Usuario

| ID | Historia | Estado | Prioridad |
|----|----------|--------|-----------|
| 63.1 | Schema Migration: scheduled_at columns | ⏳ | P0 |
| 63.2 | SQL Trigger para pg_notify | ⏳ | P0 |
| 63.3 | PgListener wrapper en Rust | ⏳ | P0 |
| 63.4 | HybridRelay con tokio::select! | ⏳ | P0 |
| 63.5 | Exponential Backoff Engine | ⏳ | P1 |
| 63.6 | Dead Letter Queue Logic | ⏳ | P1 |
| 63.7 | Saga Reaper para comandos huérfanos | ⏳ | P2 |
| 63.8 | Integration Tests | ⏳ | P1 |

---

## Historia de Usuario 63.1: Schema Migration

**Como** sistema de bases de datos  
**Quiero** que la tabla hodei_commands tenga columnas de scheduling  
**Para** soportar reintentos programados

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que verifica columna `scheduled_at` existe
- [ ] **GREEN:** Migration SQL añade columnas
- [ ] **REFACTOR:** Actualizar `PostgresCommandOutboxRepository`

### Tests TDD (Red phase):

```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn test_schema_has_scheduled_at_column() {
    let pool = setup_test_db().await;
    let repo = PostgresCommandOutboxRepository::new(pool);

    // Insertar comando
    let id = repo.insert_command(&make_insert(...)).await.unwrap();

    // Verificar que tiene scheduled_at (no NULL)
    let pending = repo.get_pending_commands(10, 5).await.unwrap();
    assert!(!pending.is_empty());
    assert!(pending[0].scheduled_at.is_some());
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn test_schema_has_retry_status() {
    let pool = setup_test_db().await;
    let repo = PostgresCommandOutboxRepository::new(pool);

    // Insertar y marcar como retry
    let id = repo.insert_command(&make_insert(...)).await.unwrap();
    repo.mark_retry(&id, "test error", 1).await.unwrap();

    // Verificar status
    let stats = repo.get_stats().await.unwrap();
    assert!(stats.retry_count > 0);
}
```

### Migration SQL:

```sql
-- Archivo: crates/server/infrastructure/migrations/YYYYMMDDHHMMSS_add_scheduled_at/up.sql

-- Añadir columnas de scheduling si no existen
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'hodei_commands' AND column_name = 'scheduled_at'
    ) THEN
        ALTER TABLE hodei_commands
        ADD COLUMN scheduled_at TIMESTAMPTZ DEFAULT NOW(),
        ADD COLUMN max_retries INTEGER DEFAULT 5,
        ADD COLUMN error_details JSONB;
    END IF;

    -- Actualizar constraint de status
    ALTER TABLE hodei_commands DROP CONSTRAINT IF EXISTS hodei_commands_status_check;
    ALTER TABLE hodei_commands ADD CONSTRAINT hodei_commands_status_check
        CHECK (status IN ('PENDING', 'RETRY', 'COMPLETED', 'FAILED', 'DEAD_LETTER', 'CANCELLED'));
END $$;

-- Trigger de notificación
CREATE OR REPLACE FUNCTION notify_outbox_insertion()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('outbox_work', NEW.id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_outbox_notify ON hodei_commands;
CREATE TRIGGER trg_outbox_notify
AFTER INSERT ON hodei_commands
FOR EACH ROW
EXECUTE FUNCTION notify_outbox_insertion();
```

---

## Historia de Usuario 63.2: SQL Trigger para pg_notify

**Como** base de datos PostgreSQL  
**Quiero** enviar notificaciones al canal 'outbox_work' cuando se inserta un comando  
**Para** despertar al relay worker de forma reactiva

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que verifica notificación recibida tras INSERT
- [ ] **GREEN:** Trigger SQL implementado
- [ ] **REFACTOR:** Test de integración end-to-end

### Tests TDD (Red phase):

```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_insert_triggers_notification() {
    // Arrange
    let pool = setup_test_db().await;
    let mut listener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .expect("Failed to create listener");
    listener.listen("outbox_work").await.expect("Failed to listen");

    let repo = PostgresCommandOutboxRepository::new(pool);

    // Act - Insertar comando (debería disparar NOTIFY)
    let insert = make_insert("TestCommand", Uuid::new_v4(), CommandTargetType::Saga, json!({}), None);
    let _id = repo.insert_command(&insert).await.unwrap();

    // Assert - Recibir notificación (con timeout)
    let notification = tokio::time::timeout(
        Duration::from_secs(5),
        listener.recv()
    ).await
    .expect("Timeout waiting for notification")
    .expect("Failed to receive notification");

    assert_eq!(notification.channel(), "outbox_work");
    // El payload contiene el UUID del comando
    let payload: String = notification.payload().unwrap_or_default();
    assert!(!payload.is_empty());
}
```

---

## Historia de Usuario 63.3: PgListener Wrapper

**Como** desarrollador  
**Quiero** un wrapper de PgListener para Rust  
**Para** manejar notificaciones de forma idiomática con tokio

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que recibe notificación con timeout
- [ ] **GREEN:** Implementación de `PgNotifyListener`
- [ ] **REFACTOR:** Integrar con `CommandOutboxRelay`

### Tests TDD (Red phase):

```rust
/// Wrapper para PostgreSQL LISTEN/NOTIFY
#[derive(Debug, Clone)]
pub struct PgNotifyListener {
    listener: sqlx::postgres::PgListener,
    rx: tokio::sync::mpsc::Receiver<OutboxNotification>,
}

#[derive(Debug, Clone)]
pub struct OutboxNotification {
    pub command_id: Uuid,
    pub payload: String,
    pub received_at: chrono::DateTime<Utc>,
}

impl PgNotifyListener {
    /// Crear nuevo listener conectado al pool
    pub async fn new(pool: &PgPool) -> Result<Self, sqlx::Error> {
        let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
        listener.listen("outbox_work").await?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        Ok(Self { listener, rx })
    }

    /// Recibir próxima notificación (con timeout)
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Option<OutboxNotification> {
        tokio::time::timeout(timeout, self.recv()).await.ok()?
    }

    /// Recibir próxima notificación
    pub async fn recv(&mut self) -> Option<OutboxNotification> {
        self.listener.recv().await.ok().map(|n| OutboxNotification {
            command_id: Uuid::parse_str(n.payload()).ok().unwrap_or_default(),
            payload: n.payload().to_string(),
            received_at: Utc::now(),
        })
    }

    /// Verificar si hay notificaciones pendientes
    pub async fn try_recv(&mut self) -> Option<OutboxNotification> {
        // Nota: PgListener no soporta try_recv, simulamos con poll_ready
        None
    }
}

#[tokio::test]
async fn test_pg_notify_listener_creation() {
    let pool = setup_test_db().await;
    let listener = PgNotifyListener::new(&pool).await.unwrap();
    assert!(listener.listener.is_valid().await);
}
```

---

## Historia de Usuario 63.4: HybridRelay con tokio::select!

**Como** sistema de relay  
**Quiero** escuchar tanto notificaciones PG como polling de seguridad  
**Para** tener reactividad inmediata con fallback confiable

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que relay procesa comando tras notificación
- [ ] **GREEN:** Implementación de `HybridCommandOutboxRelay`
- [ ] **REFACTOR:** Refactorizar `CommandOutboxRelay` existente

### Tests TDD (Red phase):

```rust
/// Relay híbrido que combina LISTEN/NOTIFY con polling
#[derive(Debug, Clone)]
pub struct HybridCommandOutboxRelay {
    repository: Arc<dyn CommandOutboxRepository>,
    command_bus: DynCommandBus,
    listener: Arc<tokio::sync::Mutex<PgNotifyListener>>,
    polling_interval: Duration,
    batch_size: usize,
    max_retries: i32,
    shutdown: tokio::sync::broadcast::Sender<()>,
}

impl HybridCommandOutboxRelay {
    /// Crear nuevo relay híbrido
    pub async fn new(
        pool: &PgPool,
        command_bus: DynCommandBus,
        polling_interval: Duration,
        batch_size: usize,
        max_retries: i32,
    ) -> Result<(Self, tokio::sync::broadcast::Receiver<()>), sqlx::Error> {
        let listener = Arc::new(tokio::sync::Mutex::new(PgNotifyListener::new(pool).await?));
        let (shutdown, rx) = tokio::sync::broadcast::channel(1);

        Ok((
            Self {
                repository: Arc::new(PostgresCommandOutboxRepository::new(pool.clone())),
                command_bus,
                listener,
                polling_interval,
                batch_size,
                max_retries,
                shutdown,
            },
            rx,
        ))
    }

    /// Ejecutar el loop del relay híbrido
    pub async fn run(&self) {
        tracing::info!(
            channel = "outbox_work",
            polling_interval_ms = self.polling_interval.as_millis(),
            batch_size = self.batch_size,
            "Starting hybrid command outbox relay"
        );

        let mut interval = tokio::time::interval(self.polling_interval);
        let mut shutdown_rx = self.shutdown.subscribe();
        let mut notification_rx = self.listener.lock().await.recv();

        loop {
            tokio::select! {
                // Canal 1: Notificación reactiva de PostgreSQL
                notification = &mut notification_rx => {
                    match notification {
                        Some(n) => {
                            tracing::trace!(command_id = %n.command_id, "Notification received");
                            // Procesar comandos pendientes
                            if let Err(e) = self.process_pending_commands().await {
                                tracing::warn!("Error processing after notification: {}", e);
                            }
                        }
                        None => {
                            // Reconectar listener
                            tracing::warn!("Listener disconnected, reconnecting...");
                            if let Ok(mut l) = self.listener.lock().await {
                                *l = PgNotifyListener::new(&self.repository.pool()).await.ok()?;
                            }
                            notification_rx = self.listener.lock().await.recv();
                            continue;
                        }
                    }
                    // Reiniciar espera de notificación
                    notification_rx = self.listener.lock().await.recv();
                }
                // Canal 2: Polling de seguridad (cada N segundos)
                _ = interval.tick() => {
                    tracing::trace!("Safety polling tick");
                    if let Err(e) = self.process_pending_commands().await {
                        tracing::warn!("Error in safety polling: {}", e);
                    }
                }
                // Canal 3: Señal de shutdown
                _ = shutdown_rx.recv() => {
                    tracing::info!("Hybrid relay shutting down");
                    break;
                }
            }
        }
    }

    /// Procesar batch de comandos pendientes
    async fn process_pending_commands(&self) -> Result<(), CommandOutboxError> {
        // FOR UPDATE SKIP LOCKED para evitar race conditions
        let commands = self.repository
            .get_pending_commands(self.batch_size, self.max_retries)
            .await?;

        if commands.is_empty() {
            return Ok(());
        }

        tracing::info!(count = commands.len(), "Processing command batch");

        for cmd in &commands {
            match self.execute_command(cmd).await {
                Ok(_) => {
                    self.repository.mark_completed(&cmd.id).await?;
                    tracing::debug!(command_id = %cmd.id, "Command completed");
                }
                Err(e) => {
                    self.handle_command_failure(cmd, &e).await?;
                }
            }
        }

        Ok(())
    }

    async fn execute_command(&self, cmd: &CommandOutboxRecord) -> Result<(), CommandOutboxError> {
        // Dispatch basado en command_type
        match cmd.command_type.as_str() {
            "MarkJobFailed" => self.dispatch_mark_job_failed(cmd).await,
            "ResumeFromManualIntervention" => self.dispatch_resume(cmd).await,
            _ => Err(CommandOutboxError::HandlerError(format!(
                "Unknown command type: {}", cmd.command_type
            ))),
        }
    }

    async fn handle_command_failure(
        &self,
        cmd: &CommandOutboxRecord,
        error: &str,
    ) -> Result<(), CommandOutboxError> {
        if cmd.retry_count >= self.max_retries {
            // Pasar a DEAD_LETTER
            self.repository.mark_dead_letter(&cmd.id, error).await?;
            tracing::error!(
                command_id = %cmd.id,
                retry_count = cmd.retry_count,
                "Command moved to DEAD_LETTER"
            );
        } else {
            // Programar retry con exponential backoff
            let delay_seconds = calculate_backoff(cmd.retry_count);
            self.repository
                .schedule_retry(&cmd.id, delay_seconds, error)
                .await?;
            tracing::warn!(
                command_id = %cmd.id,
                retry_count = cmd.retry_count,
                delay_seconds,
                "Command scheduled for retry"
            );
        }
        Ok(())
    }
}

/// Calcular delay de retry con exponential backoff + jitter
fn calculate_backoff(retry_count: i32) -> i64 {
    // Base: 5 segundos, multiplicador: 2^retry_count
    // retry_count=0 → 5s, retry_count=1 → 10s, retry_count=2 → 20s...
    // Máximo: 30 minutos (1800 segundos)
    let base_delay = 5;
    let delay = base_delay * 2i64.pow(retry_count as u32);

    // Añadir jitter ±10%
    let jitter = (delay / 10) * (rand::random::<u64>() % 20 + 90) / 100;

    delay.min(1800) // Máximo 30 minutos
}

#[tokio::test]
async fn test_calculate_backoff() {
    assert_eq!(calculate_backoff(0), 5);           // 5s
    assert_eq!(calculate_backoff(1), 10);          // 10s
    assert_eq!(calculate_backoff(2), 20);          // 20s
    assert_eq!(calculate_backoff(3), 40);          // 40s
    assert_eq!(calculate_backoff(10), 1800);       // capped at 30min
}
```

---

## Historia de Usuario 63.5: Exponential Backoff Engine

**Como** sistema de reintentos  
**Quiero** calcular delays de retry con exponential backoff y jitter  
**Para** evitar thundering herd y dar tiempo a recuperación

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test de tabla de delays esperados
- [ ] **GREEN:** Implementación de `BackoffStrategy`
- [ ] **REFACTOR:** Integrar con relay

### Tests TDD (Red phase):

```rust
/// Configuración de exponential backoff
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Delay base en segundos (default: 5)
    pub base_delay_secs: i64,
    /// Delay máximo en segundos (default: 1800 = 30min)
    pub max_delay_secs: i64,
    /// Factor de jitter (0.0 = sin jitter, 0.1 = ±10%)
    pub jitter_factor: f64,
    /// Máximo reintentos antes de dead letter
    pub max_retries: i32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base_delay_secs: 5,
            max_delay_secs: 1800, // 30 minutos
            jitter_factor: 0.1,   // ±10%
            max_retries: 5,
        }
    }
}

impl BackoffConfig {
    /// Calcular delay para un retry específico
    pub fn calculate_delay(&self, retry_count: i32) -> chrono::Duration {
        // Exponential: base * 2^retry_count
        let raw_delay = self.base_delay_secs * 2i64.pow(retry_count as u32);

        // Apply cap
        let delay = raw_delay.min(self.max_delay_secs);

        // Apply jitter
        let jitter_range = (delay as f64 * self.jitter_factor) as i64;
        let jitter = if jitter_range > 0 {
            let jitter_value = rand::random::<i64>() % jitter_range;
            -jitter_range/2 + jitter_value // ±jitter_range/2
        } else {
            0
        };

        chrono::Duration::seconds(delay + jitter)
    }

    /// Calcular timestamp de próximo retry
    pub fn next_retry_at(&self, retry_count: i32) -> chrono::DateTime<Utc> {
        Utc::now() + self.calculate_delay(retry_count)
    }
}

#[tokio::test]
async fn test_backoff_config_defaults() {
    let config = BackoffConfig::default();

    // Verificar delays
    assert_eq!(config.calculate_delay(0).num_seconds(), 5);       // 5s
    assert_eq!(config.calculate_delay(1).num_seconds(), 10);      // 10s
    assert_eq!(config.calculate_delay(2).num_seconds(), 20);      // 20s
    assert_eq!(config.calculate_delay(3).num_seconds(), 40);      // 40s

    // Verificar límite máximo
    let large_retry = config.calculate_delay(10).num_seconds();
    assert!(large_retry <= 1800); // capped at 30min
}

#[tokio::test]
async fn test_backoff_with_jitter() {
    let config = BackoffConfig {
        base_delay_secs: 100,
        jitter_factor: 0.2, // ±20%
        ..Default::default()
    };

    let delays: Vec<i64> = (0..10)
        .map(|i| config.calculate_delay(i).num_seconds())
        .collect();

    // Verificar que hay variación (jitter)
    let unique_delays: std::collections::HashSet<_> = delays.iter().collect();
    assert!(unique_delays.len() > 1, "Jitter should produce different delays");
}
```

---

## Historia de Usuario 63.6: Dead Letter Queue Logic

**Como** sistema de cola de mensajes fallidos  
**Quiero** mover comandos fallidos irrevocablemente a una cola de dead letter  
**Para** permitir intervención manual y auditoría

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que verifica comandos en DEAD_LETTER tras max_retries
- [ ] **GREEN:** Implementación de `mark_dead_letter`
- [ ] **REFACTOR:** Añadir query de dead letters para admin

### Tests TDD (Red phase):

```rust
/// Extender CommandOutboxRepository con métodos de DLQ
#[async_trait::async_trait]
pub trait DeadLetterQueue: Send + Sync {
    /// Mover comando a dead letter
    async fn mark_dead_letter(
        &self,
        command_id: &Uuid,
        error: &str,
    ) -> Result<(), CommandOutboxError>;

    /// Obtener todos los dead letters
    async fn get_dead_letters(
        &self,
        limit: usize,
    ) -> Result<Vec<CommandOutboxRecord>, CommandOutboxError>;

    /// Reprocesar un dead letter (mover de vuelta a PENDING)
    async fn retry_dead_letter(
        &self,
        command_id: &Uuid,
    ) -> Result<(), CommandOutboxError>;

    /// Obtener estadísticas de dead letters
    async fn get_dead_letter_stats(&self) -> Result<DeadLetterStats, CommandOutboxError>;
}

#[derive(Debug, Clone)]
pub struct DeadLetterStats {
    pub total_count: u64,
    pub oldest_age_seconds: Option<i64>,
    pub by_command_type: std::collections::HashMap<String, u64>,
}

#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_dead_letter_queue() {
    let pool = setup_test_db().await;
    let repo = PostgresCommandOutboxRepository::new(pool);

    // Insertar comando
    let id = repo.insert_command(&make_insert("TestCmd", ...)).await.unwrap();

    // Simular múltiples fallos
    for i in 0..6 {
        repo.mark_failed(&id, &format!("Error {}", i)).await.unwrap();
    }

    // Ahora debería estar en dead letter (6 reintentos > max=5)
    let dead_letters = repo.get_dead_letters(10).await.unwrap();
    assert!(dead_letters.iter().any(|c| c.id == id));

    // Stats
    let stats = repo.get_dead_letter_stats().await.unwrap();
    assert!(stats.total_count > 0);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_retry_dead_letter() {
    let pool = setup_test_db().await;
    let repo = PostgresCommandOutboxRepository::new(pool);

    // Crear dead letter
    let id = repo.insert_command(&make_insert("RetryableCmd", ...)).await.unwrap();
    repo.mark_dead_letter(&id, "test error").await.unwrap();

    // Verificar que está en DLQ
    let dlq = repo.get_dead_letters(10).await.unwrap();
    assert!(dlq.iter().any(|c| c.id == id));

    // Retry
    repo.retry_dead_letter(&id).await.unwrap();

    // Verificar que volvió a PENDING
    let pending = repo.get_pending_commands(10, 5).await.unwrap();
    assert!(pending.iter().any(|c| c.id == id));
}
```

---

## Historia de Usuario 63.7: Saga Reaper para Comandos Huérfanos

**Como** sistema de limpieza  
**Quiero** un componente que detecte y limpie comandos huérfanos  
**Para** evitar acumulación de comandos zombie

### Criterios de Aceptación (TDD):

- [ ] **RED:** Test que detecta comandos pendientes por > X horas
- [ ] **GREEN:** Implementación de `CommandReaper`
- [ ] **REFACTOR:** Integrar con CleanupSaga existente

### Tests TDD (Red phase):

```rust
/// Componente de limpieza para comandos huérfanos
#[derive(Debug, Clone)]
pub struct CommandReaper {
    repository: Arc<dyn CommandOutboxRepository>,
    /// Comandos pendientes por más de este tiempo serán marcados
    orphan_threshold: chrono::Duration,
    /// Intervalo de ejecución del reaper
    run_interval: chrono::Duration,
}

impl CommandReaper {
    pub fn new(
        repository: Arc<dyn CommandOutboxRepository>,
        orphan_threshold_hours: i64,
        run_interval_minutes: i64,
    ) -> Self {
        Self {
            repository,
            orphan_threshold: chrono::Duration::hours(orphan_threshold_hours),
            run_interval: chrono::Duration::minutes(run_interval_minutes),
        }
    }

    /// Buscar comandos huérfanos (pendientes por demasiado tiempo)
    async fn find_orphans(&self) -> Result<Vec<CommandOutboxRecord>, CommandOutboxError> {
        // Comandos PENDING o RETRY más antiguos que el threshold
        let threshold_time = Utc::now() - self.orphan_threshold;

        // Esta query需要一个额外的索引
        // SELECT * FROM hodei_commands WHERE status IN ('PENDING', 'RETRAY')
        // AND created_at < threshold_time
        // LIMIT 100

        todo!()
    }

    /// Marcar comandos huérfanos como fallidos con reason
    async fn reap_orphans(&self) -> Result<usize, CommandOutboxError> {
        let orphans = self.find_orphans().await?;
        let mut reaped = 0;

        for orphan in &orphans {
            self.repository
                .mark_failed(
                    &orphan.id,
                    &format!("Orphaned command - no processing in {}", self.orphan_threshold),
                )
                .await?;
            reaped += 1;
        }

        if reaped > 0 {
            tracing::warn!(count = reaped, "Reaped orphaned commands");
        }

        Ok(reaped)
    }
}

#[tokio::test]
async fn test_command_reaper_finds_orphans() {
    // Crear comando con fecha antigua
    // Verificar que reaper lo detecta
    todo!()
}
```

---

## Historia de Usuario 63.8: Integration Tests

**Como** sistema de integración  
**Quiero** tests que验证 todo el flujo híbrido  
**Para** garantizar que la arquitectura funciona end-to-end

### Tests de Integración:

```rust
#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_hybrid_relay_full_flow() {
    // Arrange
    let pool = setup_test_db().await;
    let command_bus = create_test_command_bus().await;
    let relay = HybridCommandOutboxRelay::new(
        &pool,
        command_bus,
        Duration::from_secs(30), // polling interval
        10,                       // batch size
        5,                        // max retries
    ).await.unwrap();

    // Act - Insertar comando (esto disparará pg_notify)
    let insert = make_insert(
        "MarkJobFailed",
        job_id,
        CommandTargetType::Job,
        json!({"reason": "integration test"}),
        None,
    );
    let command_id = repo.insert_command(&insert).await.unwrap();

    // Simular el procesamiento que haría el relay
    relay.process_pending_commands().await.unwrap();

    // Assert - Verificar procesamiento
    let stats = repo.get_stats().await.unwrap();
    assert_eq!(stats.completed_count, 1);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL")]
async fn test_hybrid_relay_retry_with_backoff() {
    // Arrange - CommandBus que siempre falla
    let failing_bus = Arc::new(FailingCommandBus::new());
    let relay = HybridCommandOutboxRelay::new(...).await.unwrap();

    // Act - Insertar comando
    let command_id = repo.insert_command(&make_insert(...)).await.unwrap();

    // Processar (fallará)
    relay.process_pending_commands().await.unwrap();

    // Assert - Verificar retry scheduling
    let pending = repo.get_pending_commands(10, 5).await.unwrap();
    let cmd = pending.iter().find(|c| c.id == command_id).unwrap();

    assert_eq!(cmd.status, CommandOutboxStatus::Retry);
    assert_eq!(cmd.retry_count, 1);
    assert!(cmd.scheduled_at > Utc::now()); // Programado para el futuro
}
```

---

## Resumen de Archivos Afectados

```
crates/server/domain/src/command/
├── outbox.rs                    (AMPLIAR: BackoffConfig, CommandOutboxStatus::Retry)
├── erased.rs                    (MODIFICAR: Integrar con relay)
└── jobs.rs                      (MODIFICAR: Añadir scheduled_at al insert)

crates/server/infrastructure/src/persistence/
├── command_outbox.rs            (AMPLIAR: DLQ methods, scheduled_at)
└── migrations/
    └── 20260108_add_hodei_commands_table/
        ├── up.sql               (AMPLIAR: trigger, scheduled_at)
        └── down.sql

crates/server/application/src/command/
├── relay.rs                     (CREAR: HybridCommandOutboxRelay)
└── backoff.rs                   (CREAR: BackoffConfig)

crates/server/domain/src/saga/
└── reaper.rs                    (CREAR: CommandReaper)
```

---

## Métricas de Éxito

| Métrica | Objetivo | Medición |
|---------|----------|----------|
| Latencia de notificación | < 100ms desde INSERT hasta processing | Test de integración |
| Recuperación ante caída | 100% de comandos procesados tras restart | Test de resiliencia |
| Dead letter accuracy | 0% falsos positivos en DLQ | Test de edge cases |
| Cobertura de tests | > 90% para relay y backoff | cargo test coverage |

---

## Dependencias

```
Épica 63 (Hybrid Outbox)
    │
    ├── Depende de:
    │   ├── Épica 50 (Command Bus Core) ✅
    │   └── Épica 59 (Type Erasure) ✅
    │
    └── Bloquea:
        ├── Épica 51 (Transactional Outbox) - Completará
        └── Épica 61 (Integration Tests) - Tests de integración
```

---

## Notas de Implementación

### Consideraciones de Producción

1. **Connection Management**: El PgListener mantiene una conexión dedicada. En caso de reconexión, usar exponential backoff.

2. **Multiple Instances**: Si hay múltiples relay workers, `FOR UPDATE SKIP LOCKED` asegura que cada comando sea procesado por exactamente un worker.

3. **Graceful Shutdown**: El relay debe terminar de procesar el batch actual antes de cerrar.

4. **Monitoring**: Métricas a exponer:
   - `commands_processed_total`
   - `commands_failed_total`
   - `commands_dead_letter_total`
   - `relay_wakeups_notifications_total`
   - `relay_wakeups_polling_total`

### Recomendaciones de Configuración

```yaml
# Producción recomendada
command_outbox:
  polling_interval: 30s       # Safety polling
  batch_size: 10              # Process up to 10 at a time
  max_retries: 5              # Before dead letter
  backoff:
    base_delay: 5s
    max_delay: 30m
    jitter: 0.1
  reaper:
    orphan_threshold: 24h     # Mark orphaned after 24h
    run_interval: 1h
```
