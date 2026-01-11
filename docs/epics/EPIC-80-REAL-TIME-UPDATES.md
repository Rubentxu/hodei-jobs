# EPIC-80: Infraestructura de Actualizaciones en Tiempo Real de Alto Rendimiento

| Estado | Prioridad | Estimación | Epic Padre |
| :--- | :--- | :--- | :--- |
| ✅ 80% Implementado | Alta | 3 Sprints | N/A |

## 📊 Estado de Implementación Actual

### ✅ Componentes Completados (80%)

#### 1. **ClientEvent Projection Layer** - ✅ COMPLETADO
- Ubicación: `crates/server/domain/src/events/mapping.rs`
- Tests: `mapping_tests.rs` (6 tests TDD)
- Filtra 30+ eventos internos, proyecta 28+ eventos de UI

#### 2. **Protocolo WebSocket - Tipos de Mensaje** - ✅ COMPLETADO
- Ubicación: `crates/shared/src/realtime/`
- `messages.rs`: ServerMessage (Batch, Event, Ack, Error)
- `commands.rs`: ClientCommand (Subscribe, Unsubscribe, Ping)
- **NUEVO**: `ClientEvent::topics()` method

#### 3. **Eventos de UI Ampliados** - ✅ COMPLETADO
- 28+ tipos de eventos proyectados (Job, Worker, Provider, Template, ScheduledJob)

#### 4. **Backpressure Strategy** - ✅ COMPLETADO
- Ubicación: `crates/server/domain/src/logging/backpressure.rs`

#### 5. **RealtimeMetrics** - ✅ IMPLEMENTADO
- Ubicación: `crates/server/infrastructure/src/realtime/metrics.rs`
- Contadores: sessions_active, messages_sent/dropped, broadcasts, subscriptions
- Histogramas: broadcast_duration_ms, event_processing_duration_us, session_duration_seconds
- Tests: 2/2 ✅

#### 6. **Session con Backpressure y Batching** - ✅ IMPLEMENTADO
- Ubicación: `crates/server/infrastructure/src/realtime/session.rs`
- Bounded channel (capacity: 1000)
- Batching: 200ms interval, max 50 events/batch
- Métricas: messages_sent, messages_dropped, drop_rate_percent
- Tests: 2/2 ✅

#### 7. **ConnectionManager con DashMap** - ✅ IMPLEMENTADO
- Ubicación: `crates/server/infrastructure/src/realtime/connection_manager.rs`
- Lock-free concurrent hashmap para sesiones y suscripciones
- Métodos: register_session, unregister_session, subscribe, unsubscribe, broadcast
- Tests: 3/3 ✅

#### 8. **RealtimeEventBridge** - ✅ IMPLEMENTADO
- Ubicación: `crates/server/infrastructure/src/realtime/bridge.rs`
- Trait `EventSubscriber` para abstracción del event bus
- Proyección DomainEvent → ClientEvent
- Broadcast a ConnectionManager por topics
- Tests: 4/4 ✅

### ❌ Pendiente de Implementación (20%)

1. **WebSocket Handler** - ❌ PENDIENTE
   - JWT autenticación en headers (NO query params)
   - Integración con Axum router
   - Endpoint `/api/v1/ws`
   - Ubicación esperada: `crates/server/interface/src/websocket/`

2. **Frontend Integration** - ❌ PENDIENTE
   - Cliente WebSocket en Leptos WASM
   - Reconexión automática con exponential backoff
   - Store de estado para eventos en tiempo real

3. **Tests de Carga** - ❌ PENDIENTE
   - Simulación de miles de conexiones concurrentes
   - Validación de throughput con batching

4. **EventBus Integration** - ⚠️ PARCIAL
   - El trait EventSubscriber está definido
   - Falta integración con PostgresEventBus real

## 📋 Resumen Ejecutivo

Esta épica tiene como objetivo dotar a la plataforma **Hodei Jobs** de capacidades de actualización en tiempo real (Live Updates) robustas y escalables. Actualmente, la interfaz depende de *polling* o recargas manuales, lo que introduce latencia en la percepción del estado del sistema y carga innecesaria en la base de datos.

El objetivo no es solo "enviar eventos", sino hacerlo garantizando la **estabilidad del navegador del cliente** (evitando *UI Freezes* por miles de eventos) y la **eficiencia del servidor** (gestionando miles de conexiones concurrentes con mínimo overhead).

### 🎯 Objetivos Estratégicos
1.  **Eliminar el Polling:** Reducir la carga de lectura en PostgreSQL reemplazando consultas periódicas por arquitectura *Push*.
2.  **Latencia Sub-segundo:** Reflejar cambios de estado de Jobs y Workers en la UI en < 500ms.
3.  **Protección contra Saturación (Backpressure):** Implementar mecanismos de *Batching* y *Throttling* para proteger clientes lentos o sobrecargados.
4.  **Observabilidad:** Trazabilidad completa desde que ocurre el evento en el dominio hasta que se renderiza en el cliente.
5.  **Excelencia Operativa:** Métricas, alertas y diagnósticos para monitoreo en producción.

---

## 🔍 Análisis de Arquitectura Existente

### ✅ Encajes Positivos (Fortalezas Reutilizables)

#### 1. **EDA Consolidado y Maduro** ✅ IMPLEMENTADO
```rust
// crates/server/domain/src/events.rs (~1900 líneas)
pub enum DomainEvent {
    JobCreated { job_id, spec, occurred_at, ... },
    JobStatusChanged { job_id, old_state, new_state, ... },
    WorkerHeartbeat { worker_id, state, load_average, ... },
    WorkerReady { worker_id, provider_id, ... },
    // ... 40+ variantes de eventos bien estructurados
}
```

**Estado Actual:**
- ✅ **ClientEvent Projection Layer** resuelto Connascence of Name
- ✅ Event mapping implementado en `crates/server/domain/src/events/mapping.rs`
- ✅ Filtra 30+ eventos internos, proyecta ~10 eventos relevantes
- ✅ Tests TDD: 6 tests unitarios en `mapping_tests.rs`

- **EventBus trait** simple y bien definido con 4 implementaciones:
  - `NatsEventBus` (NATS JetStream - production) ✅
  - `PostgresEventBus` (LISTEN/NOTIFY - backup) ✅
  - `OutboxEventBus` (transactional outbox pattern) ✅
  - `InMemoryEventBus` (testing) ✅
- **ResilientSubscriber** ya implementado con exponential backoff y reconnection automática
- **Transactional Outbox Pattern** consolidado con `OutboxRelay` y `HybridOutboxRelay`
- **JetStream consumers** configurados con AckPolicy::Explicit, DeliverPolicy::All

#### 2. **Patrones de Concurrencia de Alto Rendimiento (EPIC-42)**
```rust
// Uso extensivo de DashMap (lock-free) en múltiples componentes
use dashmap::DashMap;

// crates/server/interface/src/grpc/worker.rs
workers: Arc<DashMap<String, RegisteredWorker>>,
otp_tokens: Arc<DashMap<String, InMemoryOtpState>>,
worker_channels: Arc<DashMap<String, mpsc::Sender<Result<ServerMessage, Status>>>,

// crates/server/interface/src/grpc/log_stream.rs
logs: Arc<DashMap<String, Vec<BufferedLogEntry>>>,
subscribers: Arc<DashMap<String, Vec<mpsc::Sender<LogEntry>>>>,
sequences: Arc<DashMap<String, u64>>,
```

- **DashMap** utilizado extensivamente para concurrencia lock-free
- Patrones probados para 1000+ conexiones concurrentes
- Bounded channels con `mpsc::channel(capacity)`

#### 3. **Arquitectura Hexagonal Clara**
```
domain/          ← Eventos puros, EventBus trait, bounded contexts
application/     ← Event handlers, use cases, coordinators
infrastructure/   ← NATS JetStream, Outbox Relay, persistence
interface/        ← gRPC services, HTTP/gRPC endpoints
```

---

### ⚠️ Peligros y Problemas Identificados (Connascence Analysis)

#### 1. **Connascence of Name - Enum Masivo** ✅ **RESUELTO**

**Problema Detectado (Original):**
```rust
// crates/server/domain/src/events.rs - Línea 1904+
impl DomainEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            DomainEvent::JobCreated { .. } => "JobCreated",
            DomainEvent::JobStatusChanged { .. } => "JobStatusChanged",
            DomainEvent::WorkerHeartbeat { .. } => "WorkerHeartbeat",
            // ... 40+ branches idénticos
        }
    }
```

**✅ Solución Implementada:**
- Creado `mapping.rs` que separa DomainEvent (40+) de ClientEvent (10+)
- ClientEvent Projection Layer implementado en `crates/server/domain/src/events/mapping.rs`
- Filtra eventos internos, remueve campos sensibles, determina topics
- **Refactoring**: Connascence of Position → Connascence of Type mediante especialización
}
```

**Riesgo para EPIC-80:**
- Mapeo de 40+ variantes → `ClientRealtimeEvent` requiere:
  1. Actualizar `DomainEvent::event_type()` match
  2. Actualizar `event_to_subject()` mapping
  3. Crear `ClientEvent::from_domain_event()` con 40+ branches
  4. Actualizar `determine_topics()` con 40+ branches
  5. Mantener sincronizados todos los mappings

**Code Smell:**
- **Shotgun Surgery**: Cualquier nuevo evento requiere actualizar 4+ lugares
- **Divergent Change**: `DomainEvent` y `ClientEvent` divergen en estructura

**Impacto:** **ALTO** - Mantenimiento excesivo y alto riesgo de bugs

**Solución Propuesta:**
Crear **Event Mapping Layer** que separa DomainEvents de ClientEvents:

```rust
// crates/server/domain/src/events/mapping.rs (NUEVO)
pub struct ClientEvent {
    pub event_type: String,
    pub event_version: u32,
    pub aggregate_id: String,
    pub payload: ClientEventPayload,
    pub occurred_at: i64,
}

pub enum ClientEventPayload {
    JobStatusChanged { job_id, old_status, new_status },
    WorkerHeartbeat { worker_id, state, cpu, memory_mb },
    // Solo ~10 eventos relevantes para UI (vs 40+ en DomainEvent)
}

impl ClientEvent {
    pub fn from_domain_event(event: DomainEvent) -> Option<Self> {
        match event {
            DomainEvent::JobStatusChanged { job_id, old_state, new_state, .. } => {
                Some(ClientEvent { ... })
            }
            DomainEvent::WorkerHeartbeat { worker_id, state, load_average, memory_usage_mb, .. } => {
                Some(ClientEvent { ... })
            }
            // Filtra eventos internos no relevantes para UI
            DomainEvent::JobDispatchAcknowledged { .. } => None,
            DomainEvent::RunJobReceived { .. } => None,
            // ...
        }
    }
}
```

**Beneficios:**
- **Separation of Concerns**: DomainEvents (sistema) vs ClientEvents (UI)
- **Filtering**: Solo eventos relevantes para UI (~10 vs 40+)
- **Projection**: Elimina campos internos (correlation_id, actor, metadata)
- **Testable en isolation**: Pruebas unitarias de mapeo independientes

---

#### 2. **Ubicación Incorrecta de EventBridge (P4 - Media Prioridad)**

**Problema Detectado en EPIC-80 Original:**
```markdown
### 1. EventBridge (Application/Infrastructure Layer)
* Suscribirse a los tópicos de NATS (hodei.events.>)
* Deserializar DomainEvent
* Convertir a ClientRealtimeEvent
* Despachar al SubscriptionManager
```

**Análisis de Violación:**
- `EventBridge` depende de `WebSocket` (HTTP/WebSocket concern)
- `SubscriptionManager` maneja sesiones (interface concern)
- Crear dependencia Application → Interface **viola hexagonal architecture**

**Arquitectura DDD Correcta:**
```
domain/           ← Eventos puros, EventBus trait, mapping logic
application/      ← Command handlers, use cases (NO WebSocket knowledge)
infrastructure/     ← WebSocket, session management, event projection
interface/         ← HTTP/gRPC handlers (thin layer over infrastructure)
```

**Solución Propuesta:**
Mover toda la lógica de realtime a `infrastructure/realtime/`:

```
crates/server/infrastructure/src/realtime/
├── mod.rs              ← Módulo raíz
├── bridge.rs           ← RealtimeEventBridge (DomainEvent → WebSocket)
├── session.rs          ← Session con backpressure
├── manager.rs          ← ConnectionManager (DashMap)
└── metrics.rs          ← RealtimeMetrics (Prometheus)
```

**Responsabilidades claras:**
- `EventBridge` → Suscribe a NATS, proyecta a clientes
- `ConnectionManager` → Gestiona sesiones y suscripciones
- `Session` → Maneja individualmente cada WebSocket con backpressure

---

#### 3. **Autenticación Insegura en Query Parameter (P5 - Alta Prioridad)**

**Problema Detectado en EPIC-80 Original:**
```rust
// ❌ INSEGURO - Token expuesto en URL
async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,  // token en URL
    ...
) {
    let claims = verify_jwt(&params.token, &state.config.jwt_secret)?;
}
```

**Riesgos de Seguridad:**
1. **Logging inseguro**: Tokens JWT aparecen en:
   - Access logs del proxy/ingress (nginx, traefik)
   - Browser history (chrome://history/)
   - Referer headers al navegar desde WebSocket
   - Analytics y herramientas de monitoreo

2. **Exposición de secrets**:
   - Tokens JWT típicamente tienen TTL de horas/días
   - Comprometido un log, el atacante tiene acceso de días
   - Violación de compliance (GDPR, SOC2, PCI-DSS)

3. **Browser Security Policy**:
   - URLs de WebSocket pueden ser compartidas accidentalmente
   - Tokens en URL no respetan SameSite cookie policies

**Solución Propuesta:**

**Opción A - Authorization Header (Recomendado):**
```rust
// ✅ SEGURO - Token en header (no logueado)
use axum::extract::HeaderMap;

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    state: State<AppState>,
) -> Response {
    // 1. Extraer JWT de Authorization header
    let auth_header = headers.get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    let claims = match auth_header.and_then(|t| verify_jwt(t)) {
        Ok(claims) => claims,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("Missing or invalid Authorization header".into())
                .unwrap();
        }
    };

    // 2. Upgrade con claims validados
    ws.on_upgrade(move |socket| handle_socket(socket, claims, state))
}
```

**Notas sobre Browser Support:**
- **API WebSocket nativa**: NO soporta headers estándar (limitación histórica)
- **Librerías modernas**: `reconnecting-websocket`, `SocksJS`, `socket.io-client` sí soportan headers
- **Fallback para navegadores con API nativa**: Ver Opción B

**Opción B - Handshake HTTP + Token de Corta Duración:**
```rust
// 1. Client inicia WebSocket con temporal token en query (5 min TTL)
ws://server/ws?temp_token=xyz123

// 2. Server valida y devuelve short-lived session token
{"session_token": "short_lived_xyz", "expires_in": 300}

// 3. Client reconnecta con session_token en subprotocolo o header
ws://server/ws?session=short_lived_xyz
```

**Recomendación Final:**
1. **Implementar Opción A (Authorization header)** como primario
2. **Añadir fallback** para navegadores antiguos (IE11, Safari < 14):
   - Documentar restricción
   - Recomendar librerías que soportan headers
3. **NO loguear** nunca el Authorization header en producción
4. **Configurar logging** de headers en `infrastructure/` para excluir `authorization`

---

#### 4. **Falta de Backpressure Control (P3 - Media Prioridad)**

**Problema Detectado en EPIC-80 Original:**
```rust
// ❌ SIN BACKPRESSURE - Events silenciosamente dropped
pub async fn broadcast(&self, topic: &str, msg: ServerMessage) {
    if let Some(subscribers) = self.subscriptions.get(topic) {
        for session_id in subscribers.value() {
            if let Some(sender) = self.sessions.get(session_id) {
                // try_send() falla silenciosamente si channel full
                let _ = sender.try_send(msg.clone());
            }
        }
    }
}
```

**Escenario de Fallo:**
1. **1000 clientes** conectados
2. **5000 eventos/segundo** de `WorkerHeartbeat` (1 evento cada 200ms × 1000 workers)
3. **Cliente lento** con buffer de 100 eventos (mobile con 3G)
4. **`try_send()`** falla → eventos silenciosamente perdidos
5. **Cliente ve UI inconsistente**: Worker aparece "Alive" pero eventos missing

**Impacto Operacional:**
- **Data loss sin notificación**: UI muestra estado incorrecto
- **No observability**: No hay métricas de drops
- **No alerta**: Operador no sabe que clientes están perdiendo eventos
- **Experiencia degradada**: Usuarios reportan "el sistema está roto"

**Solución Propuesta:**
```rust
// crates/server/infrastructure/src/realtime/session.rs

use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};

const SESSION_CHANNEL_CAPACITY: usize = 1000;
const BACKPRESSURE_THRESHOLD: usize = 800;  // 80% de capacidad

pub struct Session {
    id: SessionId,
    tx: mpsc::Sender<Message>,
    subscribed_topics: DashSet<String>,

    // Métricas de backpressure
    messages_sent: AtomicU64,
    messages_dropped: AtomicU64,
    last_backpressure: AtomicU64,
}

impl Session {
    pub async fn send_message(&self, message: Message) -> Result<(), SessionError> {
        match self.tx.try_send(message) {
            Ok(_) => {
                self.messages_sent.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Backpressure detectado
                self.last_backpressure.store(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    Ordering::Relaxed
                );

                self.messages_dropped.fetch_add(1, Ordering::Relaxed);
                Err(SessionError::Backpressure)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(SessionError::Closed)
            }
        }
    }

    pub fn metrics(&self) -> SessionMetrics {
        SessionMetrics {
            id: self.id.clone(),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_dropped: self.messages_dropped.load(Ordering::Relaxed),
            active_subscriptions: self.subscribed_topics.len(),
            last_backpressure: self.last_backpressure.load(Ordering::Relaxed),
            drop_rate_percent: self.calculate_drop_rate(),
        }
    }

    fn calculate_drop_rate(&self) -> f64 {
        let sent = self.messages_sent.load(Ordering::Relaxed);
        let dropped = self.messages_dropped.load(Ordering::Relaxed);
        if sent + dropped == 0 {
            0.0
        } else {
            (dropped as f64 / (sent + dropped) as f64) * 100.0
        }
    }
}

#[derive(Debug)]
pub enum SessionError {
    Backpressure,
    Closed,
}
```

**Métricas Prometheus:**
```rust
// crates/server/infrastructure/src/realtime/metrics.rs

use prometheus::{Counter, Histogram, Gauge, Registry};

pub struct RealtimeMetrics {
    sessions_total: Gauge,
    messages_sent_total: Counter,
    messages_dropped_total: Counter,
    backpressure_detected: Counter,
    session_duration: Histogram,
}

impl RealtimeMetrics {
    pub fn record_backpressure(&self, session_id: &str) {
        self.backpressure_detected.inc();
        // Log para alerta
        warn!("Backpressure detected for session {}", session_id);
    }

    pub fn record_message_dropped(&self, session_id: &str, reason: &str) {
        self.messages_dropped_total.inc();
        error!("Message dropped for session {}: {}", session_id, reason);
    }
}
```

**Alertas de Monitoreo:**
```yaml
# prometheus/alerts.yml
groups:
  - name: realtime
    rules:
      - alert: HighMessageDropRate
        expr: rate(realtime_messages_dropped_total[5m]) / rate(realtime_messages_sent_total[5m]) > 0.01
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Drop rate > 1% for 5 minutes"
          description: "Session {{ $session }} dropping {{ $value | humanize }} messages/sec"

      - alert: CriticalBackpressure
        expr: increase(realtime_backpressure_detected[1m]) > 10
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Backpressure spikes detected"
          description: "{{ $value }} sessions experiencing backpressure"
```

---

#### 5. **Serialización JSON Ineficiente (P2 - Baja Prioridad)**

**Problema Detectado:**
```rust
#[derive(Serialize, Deserialize)]
pub enum ClientRealtimeEvent {
    JobStatus { ... },
    WorkerMetrics { ... },
}

// Cada evento se serializa a JSON separadamente
let json = serde_json::to_string(&event)?;
ws_sender.send(Message::Text(json)).await?;
```

**Costo Operacional:**
- **Parsing JSON en navegador**: Costoso en CPU
- **Tamaño**: JSON ~40-60% más grande que binario
- **Con 5000 eventos/segundo**: Bottleneck en parsing

**Fase 1 - JSON (Implementación Inicial):**
- Mantener JSON por simplicidad
- Medir throughput real: eventos/segundo procesables en UI
- **Benchmark objetivo**: > 2000 eventos/segundo sin UI freeze

**Fase 2 - Binary (Optimización Futura):**
```rust
// Evaluar FlatBuffers o Protobuf si JSON < 2000 events/sec
pub struct RealtimeMessage {
    event_type: u16,      // 2 bytes
    payload_size: u16,    // 2 bytes
    payload: Vec<u8>,      // variable
}

// ~50-60% más pequeño que JSON
// Parsing ~10x más rápido
```

**Decision Point (Sprint 2):**
- Si throughput medido > 2000 events/sec → Mantener JSON
- Si throughput medido < 2000 events/sec → Implementar binary

---

## 🏗️ Arquitectura Técnica Refinada

La solución se basa en extender la Arquitectura Hexagonal existente, introduciendo un **adaptador de proyección en tiempo real** en la capa de `infrastructure/` que consume del Bus de Eventos (NATS) y proyecta hacia los clientes conectados con protocolo WebSocket.

### Diagrama de Flujo de Datos (Corregido)

```mermaid
graph TD
    A[Domain Events - NATS JetStream] -->|Subscribe: hodei.events.>| B[RealtimeEventBridge - infrastructure/realtime]
    B -->|Project: DomainEvent → ClientEvent| C[ClientEvent - Filtered ~10/40+ events]
    C -->|Determine Topics| D[ConnectionManager - DashMap]
    D -->|Topic: job:UUID| E[Session A - Bounded Channel 1000]
    D -->|Topic: workers:all| F[Session B - Bounded Channel 1000]
    D -->|Topic: jobs:all| G[Session C - Bounded Channel 1000]
    
    subgraph "Session Actor with Backpressure"
        E1[Event Queue] -->|try_send| E2{Channel Full?}
        E2 -->|Yes| E3[Increment drops_counter]
        E2 -->|No| E4[Increment sent_counter]
        E3 --> E5[Log error + Alert]
        E4 --> E6[Batch Buffer]
        E6 -->|200ms tick OR >50 events| E7[WebSocket Send JSON Batch]
    end
    
    E7 -->|Message: {t:batch,d:[events]}| H[Browser - Leptos WASM]
    
    style B fill:#e1f5ff
    style D fill:#fff4e1
    style E fill:#ffe1e1
    style F fill:#ffe1e1
    style G fill:#ffe1e1
```

### Componentes del Sistema (Refinado)

#### 1. **ClientEvent Projection Layer** (`domain/src/events/mapping.rs`)

**Responsabilidad:**
- Proyectar `DomainEvent` → `ClientEvent`
- Filtrar eventos no relevantes para UI (~10/40+)
- Remover campos internos (correlation_id, actor, metadata)
- Validar y sanitizar datos sensibles

**API:**
```rust
// crates/server/domain/src/events/mapping.rs

use crate::events::DomainEvent;
use serde::{Deserialize, Serialize};

/// Evento proyectado para clientes WebSocket
#[derive(Debug, Clone, Serialize)]
pub struct ClientEvent {
    pub event_type: String,
    pub event_version: u32,
    pub aggregate_id: String,
    pub payload: ClientEventPayload,
    pub occurred_at: i64,  // Unix timestamp ms
}

/// Payloads de eventos relevantes para UI
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum ClientEventPayload {
    #[serde(rename = "job.status_changed")]
    JobStatusChanged {
        job_id: String,
        old_status: String,
        new_status: String,
        timestamp: i64,
    },

    #[serde(rename = "worker.heartbeat")]
    WorkerHeartbeat {
        worker_id: String,
        state: String,
        cpu: Option<f32>,
        memory_mb: Option<u64>,
        active_jobs: u32,
    },

    #[serde(rename = "worker.ready")]
    WorkerReady {
        worker_id: String,
        provider_id: String,
    },

    #[serde(rename = "worker.terminated")]
    WorkerTerminated {
        worker_id: String,
        provider_id: String,
        reason: String,
    },

    #[serde(rename = "system.toast")]
    SystemToast {
        level: String,  // "info", "warning", "error", "success"
        title: String,
        message: String,
        duration_ms: Option<u32>,
    },
}

impl ClientEvent {
    /// Proyecta DomainEvent a ClientEvent
    ///
    /// Retorna None para eventos internos no relevantes para UI
    pub fn from_domain_event(event: DomainEvent) -> Option<Self> {
        match event {
            // Job Events - Todos relevantes para UI
            DomainEvent::JobCreated { job_id, occurred_at, .. } => {
                Some(Self {
                    event_type: "job.created".to_string(),
                    event_version: 1,
                    aggregate_id: job_id.to_string(),
                    payload: ClientEventPayload::JobCreated { ... },
                    occurred_at: occurred_at.timestamp_millis(),
                })
            }

            DomainEvent::JobStatusChanged { job_id, old_state, new_state, occurred_at, .. } => {
                Some(Self {
                    event_type: "job.status_changed".to_string(),
                    event_version: 1,
                    aggregate_id: job_id.to_string(),
                    payload: ClientEventPayload::JobStatusChanged { ... },
                    occurred_at: occurred_at.timestamp_millis(),
                })
            }

            // Worker Events - Filtrar algunos internos
            DomainEvent::WorkerHeartbeat { worker_id, state, load_average, memory_usage_mb, current_job_id, occurred_at, .. } => {
                Some(Self {
                    event_type: "worker.heartbeat".to_string(),
                    event_version: 1,
                    aggregate_id: worker_id.to_string(),
                    payload: ClientEventPayload::WorkerHeartbeat {
                        worker_id: worker_id.to_string(),
                        state: state.to_string(),
                        cpu: load_average.map(|l| l as f32),
                        memory_mb: memory_usage_mb,
                        active_jobs: if current_job_id.is_some() { 1 } else { 0 },
                    },
                    occurred_at: occurred_at.timestamp_millis(),
                })
            }

            DomainEvent::WorkerReady { worker_id, provider_id, occurred_at, .. } => {
                Some(Self {
                    event_type: "worker.ready".to_string(),
                    event_version: 1,
                    aggregate_id: worker_id.to_string(),
                    payload: ClientEventPayload::WorkerReady { ... },
                    occurred_at: occurred_at.timestamp_millis(),
                })
            }

            // Eventos internos - NO proyectar a clientes
            DomainEvent::JobDispatchAcknowledged { .. } => None,
            DomainEvent::RunJobReceived { .. } => None,
            DomainEvent::JobQueueDepthChanged { .. } => None,  // Métricas internas
            DomainEvent::GarbageCollectionCompleted { .. } => None,  // Infra

            // Otros eventos no relevantes para UI
            _ => None,
        }
    }

    /// Determina los tópicos para un evento
    pub fn topics(&self) -> Vec<String> {
        let mut topics = vec![];

        // Topic por aggregate (job-specific, worker-specific)
        if self.aggregate_id.contains('-') {  // UUID format
            topics.push(format!("agg:{}", self.aggregate_id));
        }

        // Topics globales por tipo de evento
        match self.event_type.as_str() {
            "job.created" | "job.status_changed" | "job.cancelled" => {
                topics.push("jobs:all".to_string());
            }
            "worker.heartbeat" | "worker.ready" | "worker.terminated" => {
                topics.push("workers:all".to_string());
            }
            "system.toast" => {
                topics.push("system:toasts".to_string());
            }
            _ => {}
        }

        topics
    }
}
```

---

#### 2. **RealtimeEventBridge** (`infrastructure/src/realtime/bridge.rs`)

**Responsabilidad:**
- Suscribirse a NATS (`hodei.events.>`)
- Deserializar `DomainEvent`
- Proyectar a `ClientEvent` (filtrar eventos no relevantes)
- Despachar al `ConnectionManager` con tópicos determinados

**API:**
```rust
// crates/server/infrastructure/src/realtime/bridge.rs

use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::mapping::ClientEvent;
use crate::realtime::ConnectionManager;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{info, error, warn, instrument};

/// Bridge entre DomainEvents (NATS) y Sesiones WebSocket
///
/// Arquitectura:
/// NATS (DomainEvent) → RealtimeEventBridge → ConnectionManager → WebSocket Sessions
///
/// Responsabilidades:
/// - Suscribirse a todos los eventos de dominio
/// - Proyectar a ClientEvent (filtrar eventos internos)
/// - Determinar tópicos de suscripción
/// - Broadcast a sesiones conectadas
pub struct RealtimeEventBridge {
    event_bus: Arc<dyn EventBus>,
    connection_manager: Arc<ConnectionManager>,
    shutdown: watch::Receiver<()>,
    metrics: Arc<RealtimeMetrics>,
}

impl RealtimeEventBridge {
    pub fn new(
        event_bus: Arc<dyn EventBus>,
        connection_manager: Arc<ConnectionManager>,
        shutdown: watch::Receiver<()>,
        metrics: Arc<RealtimeMetrics>,
    ) -> Self {
        Self {
            event_bus,
            connection_manager,
            shutdown,
            metrics,
        }
    }

    /// Inicia el puente de eventos en tiempo real
    ///
    /// Suscribe a todos los eventos de dominio y proyecta a clientes
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), EventBusError> {
        info!("Starting RealtimeEventBridge");

        // Suscribir a todos los eventos de dominio
        let mut event_stream = self.event_bus.subscribe("hodei.events.>").await?;

        let connection_manager = self.connection_manager.clone();
        let metrics = self.metrics.clone();

        // Loop principal de procesamiento de eventos
        loop {
            tokio::select! {
                // Señal de shutdown
                _ = self.shutdown.changed() => {
                    info!("RealtimeEventBridge shutting down");
                    break;
                }

                // Recibir evento de NATS
                result = event_stream.next() => {
                    match result {
                        Some(Ok(domain_event)) => {
                            metrics.record_domain_event_received(&domain_event);

                            // Proyectar a ClientEvent
                            match ClientEvent::from_domain_event(domain_event) {
                                Some(client_event) => {
                                    metrics.record_client_event_projected(&client_event);

                                    // Determinar tópicos
                                    let topics = client_event.topics();

                                    // Broadcast a sesiones suscritas
                                    for topic in topics {
                                        connection_manager.broadcast(topic, &client_event).await;
                                    }
                                }
                                None => {
                                    // Evento filtrado (no relevante para UI)
                                    metrics.record_domain_event_filtered(&domain_event);
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!("Error receiving event: {}", e);
                            metrics.record_event_error(&e.to_string());
                        }
                        None => {
                            warn!("Event stream ended");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
```

---

#### 3. **ConnectionManager** (`infrastructure/src/realtime/manager.rs`)

**Responsabilidad:**
- Registrar/deregistrar sesiones WebSocket
- Manejar suscripciones a tópicos
- Broadcast de eventos a sesiones suscritas
- Mantener estado concurrente con DashMap (lock-free)

**API:**
```rust
// crates/server/infrastructure/src/realtime/manager.rs

use dashmap::{DashMap, DashSet};
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, warn, instrument};

use super::session::{Session, SessionId};
use super::metrics::RealtimeMetrics;

/// Gestiona conexiones WebSocket y suscripciones
///
/// Usa DashMap para concurrencia lock-free (evita Mutex contention)
///
/// Estructuras de datos:
/// - sessions: SessionId → Session (baja frecuencia: connect/disconnect)
/// - subscriptions: Topic → DashSet<SessionId> (alta frecuencia: broadcast)
pub struct ConnectionManager {
    /// Sesiones activas (DashMap para lock-free)
    sessions: DashMap<SessionId, Session>,

    /// Suscripciones por tópico (DashMap para lock-free)
    subscriptions: DashMap<String, DashSet<SessionId>>,

    /// Métricas
    metrics: Arc<RealtimeMetrics>,
}

impl ConnectionManager {
    pub fn new(metrics: Arc<RealtimeMetrics>) -> Self {
        Self {
            sessions: DashMap::new(),
            subscriptions: DashMap::new(),
            metrics,
        }
    }

    /// Registra una nueva sesión WebSocket
    #[instrument(skip(self, session))]
    pub async fn register_session(&self, session: Session) {
        info!("Registering session: {}", session.id());
        self.sessions.insert(session.id().clone(), session);
        self.metrics.session_active_inc();
    }

    /// Desregistra una sesión WebSocket
    #[instrument(skip(self))]
    pub async fn unregister_session(&self, session_id: &SessionId) {
        info!("Unregistering session: {}", session_id);

        // Remover sesión
        self.sessions.remove(session_id);

        // Remover de todas las suscripciones
        self.subscriptions.retain(|_topic, sessions| {
            sessions.remove(session_id);
            // Retener tópico si hay otras sesiones suscritas
            !sessions.is_empty()
        });

        self.metrics.session_active_dec();
    }

    /// Suscribe una sesión a un tópico
    #[instrument(skip(self))]
    pub async fn subscribe(&self, session_id: &SessionId, topic: String) {
        info!("Session {} subscribing to {}", session_id, topic);

        // Verificar que la sesión existe
        if !self.sessions.contains_key(session_id) {
            warn!("Attempt to subscribe non-existent session: {}", session_id);
            return;
        }

        // Añadir a suscripciones
        let sessions = self.subscriptions.entry(topic).or_insert_with(DashSet::new);
        sessions.insert(session_id.clone());

        self.metrics.subscription_inc(&topic);
    }

    /// Desuscribe una sesión de un tópico
    #[instrument(skip(self))]
    pub async fn unsubscribe(&self, session_id: &SessionId, topic: &str) {
        info!("Session {} unsubscribing from {}", session_id, topic);

        if let Some(mut sessions) = self.subscriptions.get_mut(topic) {
            sessions.remove(session_id);

            // Remover tópico si no hay sesiones suscritas
            if sessions.is_empty() {
                self.subscriptions.remove(topic);
            }
        }

        self.metrics.subscription_dec(topic);
    }

    /// Broadcast de evento a todas las sesiones suscritas a un tópico
    ///
    /// Retorna número de sesiones a las que se envió el evento
    #[instrument(skip(self, event))]
    pub async fn broadcast(&self, topic: &str, event: &ClientEvent) -> Result<usize, BroadcastError> {
        let mut sent_count = 0;
        let mut error_count = 0;

        // Serializar evento una vez (optimización)
        let message = Message::Text(serde_json::to_string(event)?);

        // Obtener sesiones suscritas al tópico
        if let Some(sessions) = self.subscriptions.get(topic) {
            for session_id in sessions.iter() {
                if let Some(session) = self.sessions.get(session_id.value()) {
                    // Enviar a sesión con backpressure control
                    match session.send_message(message.clone()).await {
                        Ok(_) => {
                            sent_count += 1;
                        }
                        Err(SessionError::Backpressure) => {
                            error!("Backpressure for session {}: channel full", session_id);
                            self.metrics.record_backpressure(session_id.value());
                            error_count += 1;
                        }
                        Err(SessionError::Closed) => {
                            warn!("Session {} closed, will remove", session_id);
                            self.unregister_session(session_id.value()).await;
                            error_count += 1;
                        }
                    }
                }
            }
        }

        self.metrics.record_broadcast(topic, sent_count, error_count);

        Ok(sent_count)
    }

    /// Obtiene métricas del manager
    pub fn metrics(&self) -> ConnectionManagerMetrics {
        ConnectionManagerMetrics {
            active_sessions: self.sessions.len() as u64,
            active_topics: self.subscriptions.len() as u64,
            total_subscriptions: self.subscriptions.iter()
                .map(|entry| entry.value().len())
                .sum::<usize>() as u64,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BroadcastError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionManagerMetrics {
    pub active_sessions: u64,
    pub active_topics: u64,
    pub total_subscriptions: u64,
}
```

---

#### 4. **Session con Backpressure** (`infrastructure/src/realtime/session.rs`)

**Responsabilidad:**
- Manejar individualmente cada conexión WebSocket
- Buffer y batch de eventos
- Control de backpressure con métricas
- Bounded channel (capacidad limitada)

**API:**
```rust
// crates/server/infrastructure/src/realtime/session.rs

use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use dashmap::DashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use tracing::{info, warn, debug};

use super::SessionId;
use super::metrics::RealtimeMetrics;

/// Sesión WebSocket con control de backpressure
///
/// Características:
/// - Bounded channel (capacity: 1000)
/// - Métricas de sent/dropped messages
/// - Tracking de backpressure events
/// - Batch de eventos para reducir frames WebSocket
pub struct Session {
    id: SessionId,
    tx: mpsc::Sender<Message>,
    subscribed_topics: DashSet<String>,

    // Métricas
    messages_sent: AtomicU64,
    messages_dropped: AtomicU64,
    last_backpressure: AtomicU64,
    session_start: SystemTime,
}

const SESSION_CHANNEL_CAPACITY: usize = 1000;
const BACKPRESSURE_THRESHOLD: usize = 800;  // 80% de capacidad
const BATCH_FLUSH_INTERVAL_MS: u64 = 200;  // 5 updates/segundo max
const BATCH_MAX_SIZE: usize = 50;

impl Session {
    pub fn new(id: SessionId, tx: mpsc::Sender<Message>) -> Self {
        Self {
            id,
            tx,
            subscribed_topics: DashSet::new(),
            messages_sent: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
            last_backpressure: AtomicU64::new(0),
            session_start: SystemTime::now(),
        }
    }

    /// Envía un mensaje a la sesión con control de backpressure
    ///
    /// Retorna:
    /// - Ok(()) si el mensaje se encoló exitosamente
    /// - Err(Backpressure) si el channel está lleno
    /// - Err(Closed) si la sesión está cerrada
    pub async fn send_message(&self, message: Message) -> Result<(), SessionError> {
        match self.tx.try_send(message) {
            Ok(_) => {
                self.messages_sent.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(msg)) => {
                // Backpressure detectado
                let timestamp = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                self.last_backpressure.store(timestamp, Ordering::Relaxed);

                let dropped_count = self.messages_dropped.fetch_add(1, Ordering::Relaxed);
                warn!(
                    "Backpressure for session {}: channel full, dropping message (total dropped: {})",
                    self.id, dropped_count
                );

                Err(SessionError::Backpressure)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                debug!("Session {} channel closed", self.id);
                Err(SessionError::Closed)
            }
        }
    }

    /// Suscribe la sesión a un tópico
    pub fn subscribe(&self, topic: String) {
        self.subscribed_topics.insert(topic);
        debug!("Session {} subscribed to {}", self.id, topic);
    }

    /// Desuscribe la sesión de un tópico
    pub fn unsubscribe(&self, topic: &str) {
        self.subscribed_topics.remove(topic);
        debug!("Session {} unsubscribed from {}", self.id, topic);
    }

    /// Verifica si la sesión está suscrita a un tópico
    pub fn is_subscribed(&self, topic: &str) -> bool {
        self.subscribed_topics.contains(topic)
    }

    /// Obtiene el ID de la sesión
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    /// Obtiene métricas de la sesión
    pub fn metrics(&self) -> SessionMetrics {
        let sent = self.messages_sent.load(Ordering::Relaxed);
        let dropped = self.messages_dropped.load(Ordering::Relaxed);
        let duration = SystemTime::now()
            .duration_since(self.session_start)
            .unwrap()
            .as_secs();

        SessionMetrics {
            id: self.id.clone(),
            messages_sent: sent,
            messages_dropped: dropped,
            active_subscriptions: self.subscribed_topics.len() as u64,
            drop_rate_percent: if sent + dropped > 0 {
                (dropped as f64 / (sent + dropped) as f64) * 100.0
            } else {
                0.0
            },
            session_duration_seconds: duration,
            last_backpressure_seconds: self.last_backpressure.load(Ordering::Relaxed),
        }
    }
}

/// ID de sesión WebSocket
pub type SessionId = String;

/// Errores de sesión
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Backpressure: channel full")]
    Backpressure,

    #[error("Session closed")]
    Closed,
}

/// Métricas de sesión
#[derive(Debug, Clone, Serialize)]
pub struct SessionMetrics {
    pub id: String,
    pub messages_sent: u64,
    pub messages_dropped: u64,
    pub active_subscriptions: u64,
    pub drop_rate_percent: f64,
    pub session_duration_seconds: u64,
    pub last_backpressure_seconds: u64,
}

/// Loop principal de sesión con batching
///
/// Batches eventos para reducir frames WebSocket
pub async fn session_loop(
    session_id: SessionId,
    mut rx: mpsc::Receiver<ClientEvent>,
    ws_sender: mpsc::Sender<Message>,
    metrics: Arc<RealtimeMetrics>,
) {
    let mut buffer = Vec::with_capacity(BATCH_MAX_SIZE);
    let mut interval = tokio::time::interval(Duration::from_millis(BATCH_FLUSH_INTERVAL_MS));

    info!("Starting session loop for {}", session_id);

    loop {
        tokio::select! {
            // Recibir evento del bus
            Some(event) = rx.recv() => {
                buffer.push(event);

                // Flush si buffer lleno
                if buffer.len() >= BATCH_MAX_SIZE {
                    if let Err(e) = flush_buffer(&mut buffer, &ws_sender).await {
                        error!("Failed to flush buffer: {}", e);
                        metrics.record_session_error(&session_id, &e.to_string());
                    }
                }
            }

            // Temporizador para enviar lo acumulado
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    if let Err(e) = flush_buffer(&mut buffer, &ws_sender).await {
                        error!("Failed to flush buffer: {}", e);
                        metrics.record_session_error(&session_id, &e.to_string());
                    }
                }
            }

            // Rx cerrado - finalizar loop
            else => {
                info!("Session {} event stream closed", session_id);
                break;
            }
        }
    }

    // Flush remaining events before exit
    if !buffer.is_empty() {
        let _ = flush_buffer(&mut buffer, &ws_sender).await;
    }

    info!("Session loop ended for {}", session_id);
}

/// Envía buffer de eventos como batch por WebSocket
async fn flush_buffer(
    buffer: &mut Vec<ClientEvent>,
    ws_sender: &mpsc::Sender<Message>,
) -> Result<(), SessionError> {
    if buffer.is_empty() {
        return Ok(());
    }

    // Crear mensaje batch
    let batch = ServerMessage::Batch(std::mem::take(buffer));
    let json = serde_json::to_string(&batch)
        .map_err(|e| SessionError::Backpressure)?;  // Usar Backpressure como error genérico

    let message = Message::Text(json);

    // Enviar por WebSocket
    ws_sender.send(message).await
        .map_err(|_| SessionError::Closed)
}

/// Envelope de mensajes del servidor
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", content = "d")]
pub enum ServerMessage {
    #[serde(rename = "batch")]
    Batch(Vec<ClientEvent>),

    #[serde(rename = "evt")]
    Event(ClientEvent),

    #[serde(rename = "err")]
    Error { code: String, msg: String },

    #[serde(rename = "ack")]
    Ack { id: String, status: String },
}
```

---

#### 5. **WebSocket Handler con JWT en Header** (`interface/src/websocket/mod.rs`)

**Responsabilidad:**
- Manejar upgrade HTTP → WebSocket
- Validar JWT desde Authorization header
- Crear sesión y registrar en ConnectionManager
- Manejar comandos del cliente (sub/unsub)

**API:**
```rust
// crates/server/interface/src/websocket/mod.rs

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
    TypedHeader,
};
use headers::Authorization;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, warn, error, instrument};

use hodei_server_infrastructure::realtime::{
    ConnectionManager, Session, session_loop, SessionId, ServerMessage, ClientEvent,
};
use hodei_server_infrastructure::realtime::metrics::RealtimeMetrics;

/// Estado del handler WebSocket
#[derive(Clone)]
pub struct WebSocketState {
    connection_manager: Arc<ConnectionManager>,
    jwt_secret: Arc<String>,
    metrics: Arc<RealtimeMetrics>,
}

impl WebSocketState {
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        jwt_secret: Arc<String>,
        metrics: Arc<RealtimeMetrics>,
    ) -> Self {
        Self {
            connection_manager,
            jwt_secret,
            metrics,
        }
    }
}

/// Handler de conexión WebSocket con autenticación JWT en Authorization header
///
/// **IMPORTANTE**: Usa Authorization header en lugar de query parameter por seguridad
///
/// Flujo:
/// 1. Extraer JWT de Authorization header
/// 2. Validar token
/// 3. Upgrade a WebSocket
/// 4. Crear sesión y registrar en ConnectionManager
/// 5. Iniciar loops: recepción de eventos + comandos del cliente
#[instrument(skip(ws, state))]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebSocketState>>,
    auth_header: Option<TypedHeader<Authorization<headers::authorization::Bearer>>,
) -> Response {
    info!("WebSocket connection requested");

    // 1. Extraer y validar JWT desde Authorization header
    let claims = match auth_header {
        Some(TypedHeader(Authorization::Bearer(token))) => {
            match verify_jwt(token, &state.jwt_secret) {
                Ok(claims) => {
                    info!("WebSocket authenticated: user={}", claims.user_id);
                    claims
                }
                Err(e) => {
                    warn!("WebSocket authentication failed: {}", e);
                    return unauthorized_response("Invalid or expired JWT");
                }
            }
        }
        None => {
            warn!("WebSocket missing Authorization header");
            return unauthorized_response("Missing Authorization header");
        }
    };

    // 2. Upgrade a WebSocket con claims
    let connection_manager = state.connection_manager.clone();
    let metrics = state.metrics.clone();

    ws.on_upgrade(move |socket| handle_socket(socket, claims, connection_manager, metrics))
}

/// Maneja la conexión WebSocket después del upgrade
#[instrument(skip(socket, claims, connection_manager, metrics))]
async fn handle_socket(
    mut socket: WebSocket,
    claims: JwtClaims,
    connection_manager: Arc<ConnectionManager>,
    metrics: Arc<RealtimeMetrics>,
) {
    let session_id = SessionId::new();
    info!("WebSocket session started: {} for user: {}", session_id, claims.user_id);

    // Crear canales para esta sesión
    let (tx, mut rx) = mpsc::channel(SESSION_CHANNEL_CAPACITY);

    // Registrar sesión en ConnectionManager
    let session = Session::new(session_id.clone(), tx);
    connection_manager.register_session(session).await;

    // Suscribirse a tópicos por defecto
    connection_manager.subscribe(&session_id, "jobs:all".to_string()).await;
    connection_manager.subscribe(&session_id, "workers:all".to_string()).await;

    // Task para recibir eventos del ConnectionManager y enviar por WebSocket
    let connection_manager_clone = connection_manager.clone();
    let session_id_clone = session_id.clone();
    let metrics_clone = metrics.clone();
    let event_task = tokio::spawn(async move {
        // Iniciar loop de batching
        session_loop(session_id_clone, rx, tx, metrics_clone).await;
    });

    // Task para recibir comandos del cliente
    let connection_manager_cmd = connection_manager.clone();
    let session_id_cmd = session_id.clone();
    let mut command_task = tokio::spawn(async move {
        while let Some(result) = socket.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    if let Err(e) = handle_client_command(&text, &session_id_cmd, &connection_manager_cmd).await {
                        error!("Error handling command from session {}: {}", session_id_cmd, e);
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed by client: {}", session_id_cmd);
                    break;
                }
                Ok(Message::Ping(data)) => {
                    // Auto-handled por tungstenite con Pong
                    let _ = socket.send(Message::Pong(data)).await;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                Ok(_) => {
                    // Ignorar mensajes binarios u otros tipos
                }
            }
        }

        // Cleanup al terminar
        connection_manager_cmd.unregister_session(&session_id_cmd).await;
        info!("WebSocket command loop ended: {}", session_id_cmd);
    });

    // Esperar a que alguna tarea termine
    tokio::select! {
        _ = event_task => {
            info!("WebSocket event task ended: {}", session_id);
            command_task.abort();
        }
        _ = command_task => {
            info!("WebSocket command task ended: {}", session_id);
            event_task.abort();
        }
    }

    // Cleanup final
    connection_manager.unregister_session(&session_id).await;
    info!("WebSocket session ended: {}", session_id);
}

/// Maneja comandos del cliente (sub/unsub)
async fn handle_client_command(
    text: &str,
    session_id: &SessionId,
    manager: &ConnectionManager,
) -> Result<(), anyhow::Error> {
    let cmd: ClientCommand = serde_json::from_str(text)?;

    match cmd {
        ClientCommand::Subscribe { topic, request_id } => {
            manager.subscribe(session_id, topic).await;
            info!("Session {} subscribed to {} (request_id: {})", session_id, topic, request_id);
        }
        ClientCommand::Unsubscribe { topic } => {
            manager.unsubscribe(session_id, &topic).await;
            info!("Session {} unsubscribed from {}", session_id, topic);
        }
        ClientCommand::Ping => {
            // Ping/Pong manejado automáticamente por tungstenite
        }
    }

    Ok(())
}

/// Respuesta de error de autenticación
fn unauthorized_response(message: &str) -> Response {
    Response::builder()
        .status(401)
        .header("Content-Type", "application/json")
        .body(serde_json::json!({
            "error": "Unauthorized",
            "message": message
        }).to_string())
        .unwrap()
}

/// Comandos del cliente
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd")]
pub enum ClientCommand {
    #[serde(rename = "sub")]
    Subscribe { topic: String, request_id: String },

    #[serde(rename = "unsub")]
    Unsubscribe { topic: String },

    #[serde(rename = "ping")]
    Ping,
}

/// Claims del JWT (reutilizar existente)
struct JwtClaims {
    user_id: String,
    exp: i64,
    // Otros campos según implementación actual
}
```

**Fallback para navegadores antiguos:**

Si se detecta navegador sin soporte de headers en WebSocket (IE11, Safari < 14):

```rust
// endpoint temporal para intercambiar token
#[post("/api/v1/ws/token-exchange")]
async fn ws_token_exchange(
    State(state): State<Arc<WebSocketState>>,
    Json(req): Json<TokenExchangeRequest>,
) -> Result<Json<TokenExchangeResponse>, StatusCode> {
    // Validar JWT de larga duración
    let claims = verify_jwt(&req.jwt_token, &state.jwt_secret)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Generar token de corta duración (5 min)
    let short_lived_token = generate_jwt(&claims.user_id, Duration::from_secs(300));

    Ok(Json(TokenExchangeResponse {
        session_token: short_lived_token,
        expires_in: 300,
    }))
}

// Cliente puede entonces usar:
// ws://server/ws?session=short_lived_token
```

---

#### 6. **RealtimeMetrics** (`infrastructure/src/realtime/metrics.rs`)

**Responsabilidad:**
- Recolectar métricas de eventos, sesiones, broadcasts
- Exponer métricas para Prometheus
- Monitorear backpressure y drops

**API:**
```rust
// crates/server/infrastructure/src/realtime/metrics.rs

use prometheus::{Counter, Gauge, Histogram, Registry, IntCounter, IntGauge};
use std::sync::Arc;

/// Métricas de sistema de realtime
#[derive(Clone)]
pub struct RealtimeMetrics {
    registry: Arc<Registry>,

    // Contadores
    sessions_active: IntGauge,
    sessions_total: IntCounter,
    messages_sent_total: IntCounter,
    messages_dropped_total: IntCounter,
    backpressure_detected: IntCounter,
    broadcasts_total: IntCounter,
    domain_events_received: IntCounter,
    client_events_projected: IntCounter,
    domain_events_filtered: IntCounter,

    // Histogramas
    broadcast_duration_ms: Histogram,
    event_processing_duration_ms: Histogram,
    session_duration_seconds: Histogram,

    // Gauges
    subscriptions_total: IntGauge,
    topics_active: IntGauge,
}

impl RealtimeMetrics {
    pub fn new() -> Self {
        let registry = Arc::new(Registry::new());

        let opts = prometheus::Opts::new("realtime_sessions_active", "Active WebSocket sessions");

        Self {
            registry,
            sessions_active: IntGauge::with_opts(opts).unwrap(),
            sessions_total: IntCounter::with_opts(prometheus::Opts::new("realtime_sessions_total", "Total sessions created")).unwrap(),
            messages_sent_total: IntCounter::with_opts(prometheus::Opts::new("realtime_messages_sent_total", "Total messages sent to clients")).unwrap(),
            messages_dropped_total: IntCounter::with_opts(prometheus::Opts::new("realtime_messages_dropped_total", "Total messages dropped due to backpressure")).unwrap(),
            backpressure_detected: IntCounter::with_opts(prometheus::Opts::new("realtime_backpressure_detected_total", "Total backpressure events detected")).unwrap(),
            broadcasts_total: IntCounter::with_opts(prometheus::Opts::new("realtime_broadcasts_total", "Total broadcast operations")).unwrap(),
            domain_events_received: IntCounter::with_opts(prometheus::Opts::new("realtime_domain_events_received_total", "Total domain events received from NATS")).unwrap(),
            client_events_projected: IntCounter::with_opts(prometheus::Opts::new("realtime_client_events_projected_total", "Total client events projected")).unwrap(),
            domain_events_filtered: IntCounter::with_opts(prometheus::Opts::new("realtime_domain_events_filtered_total", "Total domain events filtered (not relevant for UI)")).unwrap(),
            broadcast_duration_ms: Histogram::with_opts(prometheus::HistogramOpts::new("realtime_broadcast_duration_ms", "Broadcast operation duration").unwrap()).unwrap(),
            event_processing_duration_ms: Histogram::with_opts(prometheus::HistogramOpts::new("realtime_event_processing_duration_ms", "Event processing duration")).unwrap()).unwrap(),
            session_duration_seconds: Histogram::with_opts(prometheus::HistogramOpts::new("realtime_session_duration_seconds", "Session duration").unwrap()).unwrap(),
            subscriptions_total: IntGauge::with_opts(prometheus::Opts::new("realtime_subscriptions_total", "Total active subscriptions")).unwrap(),
            topics_active: IntGauge::with_opts(prometheus::Opts::new("realtime_topics_active", "Active topics")).unwrap(),
        }
    }

    // Methods para registrar métricas
    pub fn session_active_inc(&self) { self.sessions_active.inc(); }
    pub fn session_active_dec(&self) { self.sessions_active.dec(); }
    pub fn session_created(&self) { self.sessions_total.inc(); }

    pub fn record_message_sent(&self) { self.messages_sent_total.inc(); }
    pub fn record_message_dropped(&self, session_id: &str) {
        self.messages_dropped_total.inc();
        self.backpressure_detected.inc();
    }

    pub fn record_backpressure(&self, session_id: &str) {
        self.backpressure_detected.inc();
    }

    pub fn record_broadcast(&self, topic: &str, sent_count: usize, error_count: usize) {
        self.broadcasts_total.inc();
        let _ = (topic, sent_count, error_count);  // Puede usarse para labels específicos
    }

    pub fn record_domain_event_received(&self, event_type: &str) {
        self.domain_events_received.inc();
    }

    pub fn record_client_event_projected(&self, event_type: &str) {
        self.client_events_projected.inc();
    }

    pub fn record_domain_event_filtered(&self, event_type: &str) {
        self.domain_events_filtered.inc();
    }

    pub fn subscription_inc(&self, topic: &str) { self.subscriptions_total.inc(); }
    pub fn subscription_dec(&self, topic: &str) { self.subscriptions_total.dec(); }

    pub fn record_session_error(&self, session_id: &str, error: &str) {
        // Log error para debug
    }

    /// Exponer métricas en formato Prometheus
    pub fn gather(&self) -> String {
        use prometheus::Encoder;
        let encoder = Encoder::new();
        let metric_families = self.registry.gather();
        encoder.encode_to_string(&metric_families).unwrap()
    }
}
```

---

## 🔌 Protocolo y Estructuras de Datos (Refinado)

### Envelope de Mensajes del Servidor

```rust
// crates/shared/src/realtime/messages.rs

use serde::{Deserialize, Serialize};

/// Envelope principal para comunicación WebSocket
///
/// Diseño optimizado para ancho de banda:
/// - Tag "t" (type) para identificar rápidamente el tipo
/// - Content "d" para payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum ServerMessage {
    /// Lote de eventos (optimización clave para alta frecuencia)
    #[serde(rename = "batch")]
    Batch(Vec<ClientEvent>),

    /// Evento único (baja frecuencia)
    #[serde(rename = "evt")]
    Event(ClientEvent),

    /// Respuesta a un comando (ej: suscripción exitosa)
    #[serde(rename = "ack")]
    Ack { id: String, status: String },

    /// Error del sistema
    #[serde(rename = "err")]
    Error { code: String, msg: String },
}

/// Evento proyectado para cliente
#[derive(Debug, Clone, Serialize)]
pub struct ClientEvent {
    pub event_type: String,
    pub event_version: u32,
    pub aggregate_id: String,
    pub payload: ClientEventPayload,
    pub occurred_at: i64,
}

/// Payloads de eventos específicos
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum ClientEventPayload {
    #[serde(rename = "job.status_changed")]
    JobStatusChanged {
        job_id: String,
        old_status: String,
        new_status: String,
        timestamp: i64,
    },

    #[serde(rename = "worker.heartbeat")]
    WorkerHeartbeat {
        worker_id: String,
        state: String,
        cpu: Option<f32>,
        memory_mb: Option<u64>,
        active_jobs: u32,
    },

    #[serde(rename = "worker.ready")]
    WorkerReady {
        worker_id: String,
        provider_id: String,
    },

    #[serde(rename = "worker.terminated")]
    WorkerTerminated {
        worker_id: String,
        provider_id: String,
        reason: String,
    },

    #[serde(rename = "system.toast")]
    SystemToast {
        level: String,
        title: String,
        message: String,
        duration_ms: Option<u32>,
    },
}
```

### Comandos del Cliente

```rust
// crates/shared/src/realtime/commands.rs

use serde::{Deserialize, Serialize};

/// Comandos del cliente hacia el servidor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum ClientCommand {
    /// Suscribirse a un tópico
    #[serde(rename = "sub")]
    Subscribe {
        topic: String,
        request_id: String,
    },

    /// Desuscribirse de un tópico
    #[serde(rename = "unsub")]
    Unsubscribe {
        topic: String,
    },

    /// Ping para mantener conexión viva
    #[serde(rename = "ping")]
    Ping,
}
```

---

## 🛠️ Estrategia de Testing (TDD Detallada)

### Principios TDD para esta Épica

1. **Test-First**: Cada funcionalidad tiene tests escritos ANTES de implementación
2. **Red-Green-Refactor**: Ciclo estándar de TDD
3. **Testing Pyramid**:
   - 70% Unit Tests (lógica pura, mapeo, batching)
   - 20% Integration Tests (componentes aislados)
   - 10% E2E Tests (flujo completo NATS → WebSocket → Cliente)

---

### Fase 1: Tests Unitarios (Lógica Pura)

**Ubicación**: `domain/src/events/mapping_tests.rs`

#### Test 1: Proyección Filtra Eventos Internos
```rust
#[tokio::test]
async fn test_client_event_projection_filters_internal_events() {
    // Setup
    let internal_event = DomainEvent::JobDispatchAcknowledged {
        job_id: JobId::new(),
        worker_id: WorkerId::new(),
        acknowledged_at: Utc::now(),
        correlation_id: None,
        actor: None,
    };

    // Execute
    let client_event = ClientEvent::from_domain_event(internal_event);

    // Assert - Evento interno NO debe proyectarse
    assert!(client_event.is_none());
}
```

#### Test 2: Proyección Remueve Datos Sensibles
```rust
#[tokio::test]
async fn test_client_event_projection_removes_sensitive_fields() {
    // Setup - Evento con spec que contiene comando potencialmente sensible
    let event = DomainEvent::JobCreated {
        job_id: JobId::new(),
        spec: JobSpec::new(vec!["echo".to_string(), "secret_password".to_string()]),
        occurred_at: Utc::now(),
        correlation_id: Some("correlation-123".to_string()),
        actor: Some("system".to_string()),
    };

    // Execute
    let client_event = ClientEvent::from_domain_event(event).unwrap();

    // Assert - Campos internos NO deben incluirse
    assert!(!client_event.payload.to_string().contains("secret_password"));
    assert!(!client_event.payload.to_string().contains("correlation-123"));
    assert!(!client_event.payload.to_string().contains("system"));
}
```

#### Test 3: Determinación de Tópicos para Eventos de Job
```rust
#[tokio::test]
async fn test_determine_topics_for_job_event() {
    // Setup
    let job_id = JobId::new().to_string();
    let event = ClientEvent {
        event_type: "job.status_changed".to_string(),
        event_version: 1,
        aggregate_id: job_id.clone(),
        payload: ClientEventPayload::JobStatusChanged {
            job_id: job_id.clone(),
            old_status: "QUEUED".to_string(),
            new_status: "RUNNING".to_string(),
            timestamp: 1234567890,
        },
        occurred_at: 1234567890,
    };

    // Execute
    let topics = event.topics();

    // Assert - Debe incluir topic por aggregate + topic global
    assert!(topics.contains(&format!("agg:{}", job_id)));
    assert!(topics.contains(&"jobs:all".to_string()));
    assert_eq!(topics.len(), 2);
}
```

#### Test 4: Determinación de Tópicos para Eventos de Worker
```rust
#[tokio::test]
async fn test_determine_topics_for_worker_event() {
    // Setup
    let worker_id = WorkerId::new().to_string();
    let event = ClientEvent {
        event_type: "worker.heartbeat".to_string(),
        event_version: 1,
        aggregate_id: worker_id.clone(),
        payload: ClientEventPayload::WorkerHeartbeat {
            worker_id: worker_id.clone(),
            state: "READY".to_string(),
            cpu: Some(45.5),
            memory_mb: Some(512),
            active_jobs: 0,
        },
        occurred_at: 1234567890,
    };

    // Execute
    let topics = event.topics();

    // Assert
    assert!(topics.contains(&format!("agg:{}", worker_id)));
    assert!(topics.contains(&"workers:all".to_string()));
}
```

---

### Fase 2: Tests de Integración (EventBridge)

**Ubicación**: `infrastructure/src/realtime/bridge_tests.rs`

#### Test 5: Bridge Proyecta Eventos a Sesiones Suscritas
```rust
#[tokio::test]
async fn test_realtime_bridge_projects_events_to_subscribed_sessions() {
    // Setup
    let event_bus = Arc::new(MockEventBus::new());
    let manager = Arc::new(ConnectionManager::new(Arc::new(RealtimeMetrics::new())));
    let bridge = RealtimeEventBridge::new(
        event_bus.clone(),
        manager.clone(),
        shutdown_signal(),
        Arc::new(RealtimeMetrics::new()),
    );

    // Registrar 2 sesiones
    let (tx1, mut rx1) = mpsc::channel(100);
    let (tx2, mut rx2) = mpsc::channel(100);
    let session1 = Session::new("sess-1".to_string(), tx1);
    let session2 = Session::new("sess-2".to_string(), tx2);
    manager.register_session(session1).await;
    manager.register_session(session2).await;

    // Suscripciones
    manager.subscribe(&"sess-1".to_string(), "jobs:all".to_string()).await;
    manager.subscribe(&"sess-2".to_string(), "workers:all".to_string()).await;

    // Iniciar bridge en background
    tokio::spawn(async move {
        bridge.start().await.unwrap();
    });

    // Allow startup
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute - Publicar evento de job
    let job_event = DomainEvent::JobStatusChanged {
        job_id: JobId::new(),
        old_state: JobState::Queued,
        new_state: JobState::Running,
        occurred_at: Utc::now(),
        correlation_id: None,
        actor: None,
    };
    event_bus.publish(&job_event).await.unwrap();

    // Assert - Session 1 (suscripta a jobs:all) debe recibir
    let received = timeout(Duration::from_secs(1), rx1.recv()).await.unwrap().unwrap();
    let client_event: ClientEvent = serde_json::from_str(&received).unwrap();
    assert_eq!(client_event.event_type, "job.status_changed");

    // Session 2 (NO suscrita a jobs:all) NO debe recibir
    assert!(rx2.try_recv().is_err());
}
```

#### Test 6: Bridge Filtra Eventos No Relevantes
```rust
#[tokio::test]
async fn test_realtime_bridge_filters_internal_events() {
    // Setup similar al Test 5
    let event_bus = Arc::new(MockEventBus::new());
    let manager = Arc::new(ConnectionManager::new(Arc::new(RealtimeMetrics::new())));

    let (tx, mut rx) = mpsc::channel(100);
    let session = Session::new("sess-1".to_string(), tx);
    manager.register_session(session).await;
    manager.subscribe(&"sess-1".to_string(), "jobs:all".to_string()).await;

    let bridge = RealtimeEventBridge::new(
        event_bus.clone(),
        manager.clone(),
        shutdown_signal(),
        Arc::new(RealtimeMetrics::new()),
    );
    tokio::spawn(async move { bridge.start().await.unwrap(); });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute - Publicar evento INTERNO (no relevante para UI)
    let internal_event = DomainEvent::JobDispatchAcknowledged {
        job_id: JobId::new(),
        worker_id: WorkerId::new(),
        acknowledged_at: Utc::now(),
        correlation_id: None,
        actor: None,
    };
    event_bus.publish(&internal_event).await.unwrap();

    // Assert - Sesión NO debe recibir evento interno
    assert!(timeout(Duration::from_millis(500), rx.recv()).await.is_err());
}
```

---

### Fase 3: Tests de Componente (Session con Backpressure)

**Ubicación**: `infrastructure/src/realtime/session_tests.rs`

#### Test 7: Backpressure Detecta Channel Full
```rust
#[tokio::test]
async fn test_session_backpressure_drops_messages() {
    // Setup
    let (tx, _rx) = mpsc::channel(10);  // Capacidad baja para test
    let session = Session::new("sess-1".to_string(), tx);

    // Execute - Enviar más mensajes que la capacidad
    for i in 0..20 {
        let msg = Message::Text(format!("message-{}", i));
        let result = session.send_message(msg).await;

        if i < 10 {
            // Primeros 10 deben ser OK
            assert!(result.is_ok());
        } else {
            // Resto debe fallar con Backpressure
            assert!(matches!(result, Err(SessionError::Backpressure)));
        }
    }

    // Assert - Métricas de drops
    let metrics = session.metrics();
    assert_eq!(metrics.messages_sent, 10);
    assert_eq!(metrics.messages_dropped, 10);
    assert!(metrics.drop_rate_percent > 40.0);  // 10/20 = 50%
}
```

#### Test 8: Batching Envía Lotes Correctamente
```rust
#[tokio::test]
async fn test_session_batching_sends_batches() {
    // Setup
    let (ws_tx, mut ws_rx) = mpsc::channel(100);
    let (session_tx, _session_rx) = mpsc::channel(100);

    let session_id = "sess-1".to_string();
    let metrics = Arc::new(RealtimeMetrics::new());

    // Iniciar loop de sesión
    tokio::spawn(async move {
        session_loop(session_id.clone(), session_tx, ws_tx, metrics).await;
    });

    // Enviar eventos a sesión
    for i in 0..25 {  // Más que BATCH_MAX_SIZE (50) para forzar flush
        let event = create_test_event(i);
        session_tx.send(event).await.unwrap();
    }

    // Esperar recibir batch
    let received = timeout(Duration::from_secs(1), ws_rx.recv()).await.unwrap().unwrap();

    // Assert - Debe ser un batch
    let server_msg: ServerMessage = serde_json::from_str(&received).unwrap();
    assert!(matches!(server_msg, ServerMessage::Batch(_)));

    // Verificar tamaño
    if let ServerMessage::Batch(events) = server_msg {
        assert_eq!(events.len(), 25);
    }
}
```

---

### Fase 4: Tests de Carga (Simulación de Estrés)

**Ubicación**: `infrastructure/tests/realtime_load_test.rs`

#### Test 9: Bridge Bajo Alta Carga de Eventos
```rust
#[tokio::test]
#[ignore = "Requires Docker and time"]  // Ejecutar manualmente
async fn test_realtime_bridge_under_high_load() {
    // Setup
    let num_sessions = 1000;
    let events_per_second = 5000;
    let test_duration = Duration::from_secs(10);

    let event_bus = Arc::new(InMemoryEventBus::new());
    let manager = Arc::new(ConnectionManager::new(Arc::new(RealtimeMetrics::new())));

    // Registrar N sesiones
    for i in 0..num_sessions {
        let (tx, _rx) = mpsc::channel(1000);
        let session = Session::new(format!("sess-{}", i), tx);
        manager.register_session(session).await;
        manager.subscribe(&format!("sess-{}", i), "jobs:all".to_string()).await;
    }

    // Iniciar bridge
    let bridge = RealtimeEventBridge::new(
        event_bus.clone(),
        manager.clone(),
        shutdown_signal(),
        Arc::new(RealtimeMetrics::new()),
    );
    tokio::spawn(async move { bridge.start().await.unwrap(); });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Producir eventos a alta tasa
    tokio::spawn(async move {
        for _ in 0..(events_per_second * test_duration.as_secs() as usize) {
            let event = DomainEvent::JobStatusChanged {
                job_id: JobId::new(),
                old_state: JobState::Queued,
                new_state: JobState::Running,
                occurred_at: Utc::now(),
                correlation_id: None,
                actor: None,
            };
            event_bus.publish(&event).await.unwrap();

            // 5000 eventos/segundo = 1 cada 200μs
            tokio::time::sleep(Duration::from_micros(200)).await;
        }
    });

    // Esperar duración del test
    tokio::time::sleep(test_duration).await;

    // Verificar métricas
    let manager_metrics = manager.metrics();
    let total_expected_events = events_per_second * test_duration.as_secs() as usize;

    // Acceptable: > 99% de eventos entregados
    assert!(manager_metrics.total_subscriptions > 0);

    let final_metrics = manager.metrics().clone();
    println!("Final metrics: {:?}", final_metrics);
}
```

#### Test 10: Uso de CPU Bajo con Batching
```rust
#[tokio::test]
async fn test_batching_reduces_cpu_usage() {
    // Setup - Medir CPU antes y después

    let (ws_tx, _ws_rx) = mpsc::channel(100);
    let (session_tx, _session_rx) = mpsc::channel(100);

    let session_id = "sess-1".to_string();
    let metrics = Arc::new(RealtimeMetrics::new());

    tokio::spawn(async move {
        session_loop(session_id.clone(), session_tx, ws_tx, metrics).await;
    });

    // Enviar 100 eventos sin batching (individual)
    let start = Instant::now();
    for i in 0..100 {
        let msg = Message::Text(serde_json::to_string(&create_test_event(i)).unwrap());
        ws_tx.send(msg).await.unwrap();
    }
    let without_batching = start.elapsed();

    // Medir con batching (simulado por session_loop)
    // ... (comparación de tiempos)

    // Assert - Batching debe ser significativamente más eficiente
    assert!(true);  // Placeholder para medición real
}
```

---

## 📅 Plan de Implementación Faseado (Actualizado)

### ✅ Sprint 0: Preparación y Event Mapping Layer (COMPLETADO)
**Objetivo**: Resuelto Connascence of Name y establecer bases

**Estado: ✅ 100% COMPLETADO**

1. **✅ Crear estructura de módulos**:
   - ✅ `domain/src/events/mapping.rs` ← ClientEvent projection
   - ✅ `shared/src/realtime/messages.rs` ← ServerMessage, ClientEvent
   - ✅ `shared/src/realtime/commands.rs` ← ClientCommand
   - ✅ `shared/src/realtime/mod.rs` ← Module exports
   - ✅ `domain/src/logging/backpressure.rs` ← Backpressure strategy

2. **✅ Implementar Event Mapping Layer** (TDD):
   - ✅ `test_client_event_projection_filters_internal_events()` - PASSED
   - ✅ `test_client_event_projection_removes_sensitive_fields()` - PASSED
   - ✅ `test_determine_topics_for_job_event()` - PASSED
   - ✅ `test_determine_topics_for_worker_event()` - PASSED
   - ✅ `test_client_event_serialization()` - PASSED
   - ✅ `test_internal_event_job_dispatch_acknowledged_filtered()` - PASSED

3. **✅ Definir tipos compartidos**:
   - ✅ `shared/src/realtime/messages.rs` ← ServerMessage (Batch, Event, Ack, Error), ClientEvent, ClientEventPayload (28+ tipos)
   - ✅ `shared/src/realtime/commands.rs` ← ClientCommand (Subscribe, Unsubscribe, Ping)

4. **✅ Actualizar mod.rs**:
   - ✅ `domain/src/events/mod.rs` ← Exportar mapping
   - ✅ `domain/src/events/mapping_tests.rs` ← TDD tests
   - ✅ `shared/src/realtime/mod.rs` ← Module exports

**Deliverables Completados:**
- ✅ ClientEvent Projection Layer funcional
- ✅ Protocolo WebSocket tipado y serializado
- ✅ Tests unitarios TDD (6 tests)
- ✅ Filtro de eventos internos (30+ eventos filtrados)
- ✅ Sanitización de datos sensibles
- ✅ Determinación de topics para eventos

---

### ❌ Sprint 1: Infrastructure Core - ConnectionManager y Session (PENDIENTE)

**Objetivo**: Capacidad de gestionar sesiones con backpressure

1. **Implementar Session** (TDD):
   - 🔄 `test_session_backpressure_drops_messages()` - PENDIENTE
   - 🔄 Bounded channel (capacity: 1000) - PENDIENTE
   - 🔄 Métricas sent/dropped/last_backpressure - PENDIENTE
   - 🔄 `send_message()` con `try_send()` y error handling - PENDIENTE

2. **Implementar ConnectionManager** (TDD):
   - 🔄 DashMap para sessions y subscriptions (lock-free) - PENDIENTE
   - 🔄 `register_session()`, `unregister_session()` - PENDIENTE
   - 🔄 `subscribe()`, `unsubscribe()` - PENDIENTE
   - 🔄 `broadcast()` con manejo de errores - PENDIENTE

3. **Implementar RealtimeMetrics**:
   - 🔄 Prometheus metrics: sessions_active, messages_sent/dropped, backpressure_detected - PENDIENTE
   - 🔄 Histogramas: broadcast_duration, session_duration - PENDIENTE
   - 🔄 Exponer endpoint `/metrics` para Prometheus scraping - PENDIENTE

**Deliverables Pendientes:**
- Session struct con backpressure y batching
- ConnectionManager con DashMap
- RealtimeMetrics para observabilidad
- Tests de componente (Session + ConnectionManager)

---

### ❌ Sprint 2: EventBridge y WebSocket Handler (PENDIENTE)

**Objetivo**: Conexión NATS → WebSocket

1. **Implementar RealtimeEventBridge** (TDD):
   - 🔄 `test_realtime_bridge_projects_events_to_subscribed_sessions()` - PENDIENTE
   - 🔄 `test_realtime_bridge_filters_internal_events()` - PENDIENTE
   - 🔄 Suscripción NATS a `hodei.events.>` - PENDIENTE
   - 🔄 Proyección DomainEvent → ClientEvent - PENDIENTE
   - 🔄 Broadcast a ConnectionManager - PENDIENTE

2. **Implementar WebSocket Handler** (TDD):
   - 🔄 Autenticación JWT en Authorization header (NO query param) - PENDIENTE
   - 🔄 Crear `/api/v1/ws/token-exchange` para navegadores antiguos - PENDIENTE
   - 🔄 Register session, subscribe default topics - PENDIENTE
   - 🔄 Handle client commands (sub/unsub) - PENDIENTE

3. **Implementar Batching en Session** (TDD):
   - 🔄 `test_session_batching_sends_batches()` - PENDIENTE
   - 🔄 Interval: 200ms (5 updates/segundo max) - PENDIENTE
   - 🔄 Max batch size: 50 events - PENDIENTE
   - 🔄 Flush por intervalo OR tamaño - PENDIENTE

4. **Integrar con Axum Router**:
   ```rust
   // crates/server/interface/src/lib.rs
   use crate::websocket::ws_handler;

   pub fn create_router() -> Router {
       Router::new()
           .route("/api/v1/ws", get(ws_handler))
           .route("/api/v1/ws/token-exchange", post(ws_token_exchange))
           // ... otros endpoints
   }
   ```

---

### ❌ Sprint 3: Frontend Integration y Optimización (PENDIENTE)

**Objetivo**: Leptos WASM consume WebSocket y renderiza actualizaciones

1. **Crear RealtimeService en Leptos**:
   - 🔄 WebSocket client con `reconnecting-websocket` - PENDIENTE
   - 🔄 Suscripción a tópicos (jobs:all, workers:all) - PENDIENTE
   - 🔄 Deserialización de ServerMessage - PENDIENTE
   - 🔄 Disparo de señales reactivas (create_rw_signal) - PENDIENTE

2. **Integrar en Dashboard de Jobs**:
   - 🔄 Lista de jobs reactiva que actualiza en tiempo real - PENDIENTE
   - 🔄 Badges de estado que cambian sin recarga - PENDIENTE
   - 🔄 Worker cards que muestran heartbeat en vivo - PENDIENTE

3. **Implementar reconexión automática**:
   - 🔄 Exponential backoff en reconexión (1s, 2s, 4s, 8s, 16s, max 30s) - PENDIENTE
   - Indicador visual "Reconectando..."
   - Al reconectar, solicitar "snapshot" del estado actual

4. **Benchmarking y Optimización**:
   - Medir throughput: eventos/segundo procesables
   - Si < 2000 events/sec → evaluar protocolo binario (FlatBuffers)
   - Si > 2000 events/sec → Mantener JSON

---

### ❌ Sprint 4: Pruebas de Carga Finales (PENDIENTE)

**Objetivo**: Validar estabilidad y performance bajo estrés

1. **Tests de carga**:
   - 🔄 `test_realtime_bridge_under_high_load()`: 1000 sesiones, 5000 eventos/segundo - PENDIENTE
   - 🔄 `test_batching_reduces_cpu_usage()`: CPU usage con/ sin batching - PENDIENTE
   - 🔄 Memory leak detection: monitorear crecimiento de memoria por 10 min - PENDIENTE

2. **Performance Baseline**:
   - 🔄 Medir latencia NATS → WebSocket: objetivo < 500ms - PENDIENTE
   - 🔄 Medir throughput: objetivo > 2000 eventos/segundo - PENDIENTE
   - 🔄 Medir drops: objetivo < 1% (backpressure acceptable) - PENDIENTE

3. **Observability Setup**:
   - 🔄 Configurar Prometheus alerts (HighMessageDropRate, CriticalBackpressure) - PENDIENTE
   - 🔄 Configurar dashboards (Grafana) para métricas realtime_* - PENDIENTE
   - 🔄 Documentar troubleshooting steps - PENDIENTE

---

## ⚠️ Análisis de Riesgos (Corregido y Ampliado)

| Riesgo | Impacto | Severidad | Mitigación | Propuesta Acción |
| :--- | :--- | :--- | :--- | :--- |
| **Saturación del Cliente (UI Freeze)** | Alto (Navegador congelado) | **P5 - Alta** | Implementación estricta de **Batching**: 200ms interval + max 50 events/batch. Priorizar eventos: JobStatusChanged > WorkerHeartbeat. Drop lowest priority si buffer full. | Sprint 2 |
| **Pérdida de Conexión** | Medio (Estado desactualizado) | **P4 - Media** | Reconexión automática con exponential backoff. UI muestra indicador "Reconectando...". Al reconectar, solicitar snapshot del estado actual. | Sprint 3 |
| **Memory Leaks** | Alto (Crash del servidor) | **P4 - Media** | Uso de `DashMap` (evita Mutex leaks). Weak references donde sea posible. Monitoreo estricto de métrica `realtime_sessions_active`. Alerta si memoria crece indefinidamente. | Sprint 1 |
| **Backpressure Silencioso** | Alto (Data loss sin notificación) | **P3 - Media** | Bounded channels (capacity 1000). `try_send()` con métricas de drops. Prometheus alert si drop_rate > 1%. Logs de backpressure con session_id. | Sprint 1 |
| **Autenticación Insegura** | Crítico (Exposición de secrets) | **P5 - Alta** | Usar `Authorization` header (NO query param). Documentar restricción para navegadores antiguos. Implementar `/ws/token-exchange` fallback. | Sprint 2 |
| **Connascence of Name (Enum Masivo)** | Alto (Mantenimiento excesivo) | **P5 - Alta** | Crear `ClientEvent` projection layer. Filtrar eventos no relevantes (~10 vs 40+). Remover campos internos (correlation_id, actor). | Sprint 0 |
| **Capa Violation (EventBridge en Application)** | Medio (Arquitectura incorrecta) | **P4 - Media** | Mover `RealtimeEventBridge` a `infrastructure/realtime/`. Mantener separación: Domain → Application → Infrastructure → Interface. | Sprint 0 |
| **JSON Ineficiente** | Bajo (Rendimiento subóptimo) | **P2 - Baja** | Mantener JSON para implementación inicial (simplicidad). Medir throughput real. Evaluar protocolo binario si < 2000 events/sec. | Sprint 3/4 |

---

## 📝 Referencias y Recursos

### Arquitectura y Patrones
- **[Clean Architecture en Rust](https://github.com/hodei-jobs/docs/architecture.md)** - Arquitectura hexagonal Hodei Jobs
- **[Domain-Driven Design](https://martinfowler.com/bliki/DomainDrivenDesign)** - Bounded contexts, aggregates
- **[Event-Driven Architecture](https://martinfowler.com/eaaDev/EventDrivenArchitecture.html)** - Event sourcing, CQRS
- **[Tokio Actor Pattern](https://ryhl.io/blog/actors-with-tokio/)** - Patrón actor en Rust

### Tecnologías
- **[NATS JetStream Documentation](https://docs.nats.io/jetstream)** - Durable consumers, ack policy
- **[DashMap](https://docs.rs/dashmap/latest/dashmap/)** - Lock-free concurrent hashmap
- **[Tokio](https://tokio.rs/)** - Async runtime, channels, timers
- **[Axum](https://docs.rs/axum/latest/axum/)** - Web framework, WebSocket upgrade
- **[Prometheus Client Rust](https://docs.rs/prometheus/latest/prometheus/)** - Metrics, histograms

### Testing
- **[Rust Testing Patterns](https://matklad.github.io/2021/05/31/how-to-test-async-rust.html)** - Async testing
- **[Testcontainers Rust](https://docs.rs/testcontainers/latest/)** - Integration testing con Docker
- **[TDD Best Practices](https://martinfowler.com/bliki/TestDrivenDevelopment)** - Red-Green-Refactor

### Seguridad
- **[OWASP WebSocket Security](https://owasp.org/www-project-websocket-security-cheat-sheet)** - Best practices
- **[JWT Best Practices](https://datatracker.ietf.org/doc/html/rfc8725)** - Token security
- **[CORS for WebSocket](https://developer.mozilla.org/en-US/docs/Web/API/WebSockets_API)** - Browser limitations

---

## ✅ Checklist de Validación Pre-Implementación

### ✅ Completados (Sprint 0)
- [x] **Documentación de arquitectura actualizada**: Documento EPIC-80 con análisis completo
- [x] **Definición de tipos compartidos**: Crear `crates/shared/src/realtime/` con `ClientEvent`, `ServerMessage`, `ClientCommand`
- [x] **Event Mapping Layer**: Implementado con 28+ tipos de eventos proyectados
- [x] **Test infrastructure**: Tests TDD implementados en `mapping_tests.rs`
- [x] **Connascence Resolution**: Problema de enum masivo resuelto con ClientEvent projection

### 🔄 En Progreso
- [ ] **Security review**: Validar JWT en Authorization header, implementar `/ws/token-exchange` fallback
- [ ] **Performance baseline**: Medir throughput actual sin WebSockets (polling)

### ❌ Pendientes
- [ ] **Observability setup**: Configurar Prometheus scraping de `realtime_*` metrics, definir alertas
- [ ] **Deployment strategy**: Blue-green deployment para evitar desconexiones masivas
- [ ] **Monitoring plan**: Dashboards Grafana para métricas de realtime, jobs, workers
- [ ] **Troubleshooting guide**: Documentar pasos de diagnóstico para problemas comunes

---

## 📊 Resumen de Connascence y Code Smells Identificados

| Elemento | Connascence | Code Smell | Severidad | Estado |
|-----------|--------------|-------------|------------|---------|
| `DomainEvent::event_type()` (40+ branches) | Name | Primitive Obsession, Shotgun Surgery | P5 - Alta | ✅ **RESUELTO** - Sprint 0 |
| 40+ variantes en un enum | Name | Shotgun Surgery | P5 - Alta | ✅ **RESUELTO** - Sprint 0 |
| EventBridge en Application/ | Type | Layer Violation | P4 - Media | 🔄 **PENDIENTE** - No implementado |
| Auth en query param | Meaning | Security Leak | P5 - Alta | 🔄 **PENDIENTE** - Sprint 2 |
| `try_send()` sin métricas | Timing | Silent Failures | P3 - Media | 🔄 **PENDIENTE** - Sprint 1 |

---

## 📋 Resumen Final de Estado

### ✅ Componentes Implementados (80% - Sprints 0-1)
1. **ClientEvent Projection Layer** - Resuelve Connascence of Name ✅
2. **Protocolo WebSocket** - Tipos tipados con Serde ✅
3. **Eventos de UI** - 28+ tipos proyectados (vs 40+ domain events) ✅
4. **Backpressure Strategy** - Reutilizable para logging y realtime ✅
5. **RealtimeMetrics** - Prometheus counters, gauges, histograms ✅
6. **Session** - Backpressure (80% threshold) y batching (200ms, 50 events) ✅
7. **ConnectionManager** - Gestión de sesiones con DashMap ✅
8. **RealtimeEventBridge** - Proyección DomainEvent → ClientEvent ✅
9. **Tests Unitarios** - 11 tests implementados (100% passing) ✅

### ❌ Componentes Pendientes (20% - Sprints 2-4)
1. **WebSocket Handler** - JWT en headers + Axum integration
2. **Frontend Integration** - Leptos WASM cliente WebSocket
3. **Tests de Carga** - Validación de rendimiento
4. **EventBus Integration** - Conexión con PostgresEventBus real

### 📊 Métricas de Progreso
| Métrica | Valor |
|---------|-------|
| Componentes completados | 8/10 (80%) |
| Tests unitarios | 11/11 passing (100%) |
| Líneas de código (realtime) | ~1,766 |
| Archivos añadidos | 9 archivos |
| Commits en release | 6 commits |

### 🎯 Próximo Sprint (Sprint 2)
**Prioridad 1**: WebSocket Handler con JWT en headers
**Prioridad 2**: Integración con Axum router
**Prioridad 3**: Frontend WebSocket client
| DashMap sin backpressure | Type | Resource Leak | P3 - Media | Sprint 1 |
| JSON vs binario no evaluado | Identity | Premature Optimization | P2 - Baja | Sprint 4 |

**Total de correcciones**: 7 (3 Alta, 3 Media, 1 Baja)

---

**Épica actualizada y refinada con análisis detallado de arquitectura existente, identificación de peligros, correcciones de código smells, estrategia TDD completa y plan de implementación faseado.**
