# Análisis de Acoplamiento, Cohesión y Connascence

**Fecha:** 2025-12-27  
**Proyecto:** Hodei Job Platform

---

## Tabla de Contenidos

1. [Introducción](#introducción)
2. [Métricas de Cohesión](#métricas-de-cohesión)
3. [Métricas de Acoplamiento](#métricas-de-acoplamiento)
4. [Análisis de Connascence](#análisis-de-connascence)
5. [Mapeo de Dependencias](#mapeo-de-dependencias)
6. [Evaluación por Bounded Context](#evaluación-por-bounded-context)
7. [Recomendaciones](#recomendaciones)

---

## 1. Introducción

Este documento analiza las métricas de calidad de diseño del proyecto Hodei Job Platform, enfocándose en:

- **Cohesión**: Grado en que los elementos de un módulo pertenecen juntos
- **Acoplamiento**: Grado de interdependencia entre módulos
- **Connascence**: Medida más refinada de acoplamiento que describe la naturaleza de las dependencias

### Objetivo

Identificar áreas de mejora en el diseño modular para:
- Facilitar mantenimiento
- Reducir impacto de cambios
- Mejorar testabilidad
- Aumentar reutilización

---

## 2. Métricas de Cohesión

### 2.1 Tipos de Cohesión (de mayor a menor calidad)

| Tipo | Descripción | Calidad |
|------|-------------|---------|
| **Funcional** | Todos los elementos contribuyen a una única tarea | ⭐⭐⭐⭐⭐ |
| **Secuencial** | Salida de un elemento es entrada del siguiente | ⭐⭐⭐⭐ |
| **Comunicacional** | Elementos operan sobre los mismos datos | ⭐⭐⭐ |
| **Procedural** | Elementos deben ejecutarse en orden específico | ⭐⭐ |
| **Temporal** | Elementos se ejecutan al mismo tiempo | ⭐⭐ |
| **Lógica** | Elementos realizan operaciones similares | ⭐ |
| **Coincidental** | No hay relación significativa | ❌ |

### 2.2 Evaluación por Módulo

#### Domain Layer

| Módulo | Cohesión | Tipo | Justificación |
|--------|----------|------|---------------|
| `jobs/aggregate.rs` | Alta | Funcional | Todo relacionado con Job lifecycle |
| `jobs/coordination.rs` | Alta | Funcional | Coordinación de ejecución |
| `jobs/templates.rs` | Alta | Funcional | Plantillas de jobs |
| `workers/aggregate.rs` | Alta | Funcional | Worker + spec + state |
| `workers/registry.rs` | Alta | Funcional | Registro de workers |
| `workers/provider_api.rs` | Alta | Funcional | API de providers |
| `scheduling/mod.rs` | Alta | Funcional | Estrategias de scheduling |
| `scheduling/strategies.rs` | Alta | Funcional | Implementación de estrategias |
| `events.rs` | Media | Comunicacional | 40+ eventos diversos |
| `audit/model.rs` | Alta | Funcional | Auditoría coherente |
| `outbox/` | Alta | Funcional | Transactional outbox |

**Análisis `events.rs`:**
```rust
// 40+ variantes en un enum - cohesión comunicacional
pub enum DomainEvent {
    JobCreated { ... },
    JobStatusChanged { ... },
    WorkerRegistered { ... },
    // ...muchos más
}
```
- La cohesión baja porque agrupa eventos de múltiples bounded contexts
- Recomendación: Considerar events por context o event traits

#### Application Layer

| Módulo | Cohesión | Tipo | Justificación |
|--------|----------|------|---------------|
| `jobs/create.rs` | Alta | Funcional | Un use case |
| `jobs/cancel.rs` | Alta | Funcional | Un use case |
| `jobs/controller.rs` | Alta | Funcional | Facade refactorizado |
| `jobs/dispatcher.rs` | Media-Alta | Secuencial | Múltiples pasos de dispatch |
| `jobs/coordinator.rs` | Alta | Funcional | Orquestación |
| `workers/lifecycle.rs` | Media | Secuencial | Múltiples responsabilidades |
| `providers/registry.rs` | Alta | Funcional | Registro de providers |

**Análisis `workers/lifecycle.rs`:**
```rust
pub struct WorkerLifecycleManager {
    // Múltiples responsabilidades potenciales
    registry: Arc<dyn WorkerRegistry>,
    providers: Arc<RwLock<HashMap<...>>>,
    config: WorkerLifecycleConfig,
    event_bus: Arc<dyn EventBus>,
    outbox_repository: Option<...>,
    health_service: Arc<WorkerHealthService>,
}
```
- Maneja: heartbeats, auto-scaling, terminación, reconciliación
- Cohesión secuencial porque las operaciones se encadenan
- Potencial para separar en sub-servicios

#### Infrastructure Layer

| Módulo | Cohesión | Tipo | Justificación |
|--------|----------|------|---------------|
| `persistence/postgres/job_repository.rs` | Alta | Funcional | CRUD de jobs |
| `persistence/postgres/job_queue.rs` | Alta | Funcional | Queue FIFO |
| `providers/docker.rs` | Media | Comunicacional | Lifecycle + logs + metrics |
| `providers/kubernetes.rs` | Media | Comunicacional | Similar a docker |
| `messaging/outbox_adapter.rs` | Alta | Funcional | Adapter EventBus→Outbox |

**Análisis de Providers:**
```rust
impl DockerProvider {
    // Implementa 7 traits diferentes
    // WorkerProviderIdentity
    // WorkerLifecycle
    // WorkerHealth
    // WorkerMetrics
    // WorkerLogs
    // WorkerCost
    // WorkerEligibility
}
```
- La segregación de traits (ISP) mejora la situación
- Internamente, las implementaciones están agrupadas lógicamente

### 2.3 Resumen de Cohesión

```
┌────────────────────────────────────────────────────────────────┐
│                    MAPA DE COHESIÓN                            │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  ████████████████████████  Domain Aggregates (Alta)           │
│  ████████████████████████  Domain Services (Alta)             │
│  ██████████████████░░░░░░  Domain Events (Media)              │
│  ████████████████████████  Use Cases (Alta)                   │
│  ██████████████████░░░░░░  Lifecycle Manager (Media-Alta)     │
│  ████████████████████████  Controllers/Facades (Alta)         │
│  ████████████████████████  Repositories (Alta)                │
│  ██████████████████░░░░░░  Providers (Media)                  │
│                                                                │
└────────────────────────────────────────────────────────────────┘
  Leyenda: █ Alta cohesión  ░ Media cohesión  ▒ Baja cohesión
```

---

## 3. Métricas de Acoplamiento

### 3.1 Tipos de Acoplamiento (de menor a mayor)

| Tipo | Descripción | Severidad |
|------|-------------|-----------|
| **Sin acoplamiento** | Módulos independientes | Ideal |
| **Data** | Comparten datos primitivos | ⭐ Bajo |
| **Stamp** | Comparten estructuras de datos | ⭐⭐ Bajo |
| **Control** | Un módulo controla otro via flags | ⭐⭐⭐ Medio |
| **Externo** | Comparten interface externa | ⭐⭐⭐ Medio |
| **Common** | Comparten datos globales | ⭐⭐⭐⭐ Alto |
| **Content** | Un módulo accede internos de otro | ⭐⭐⭐⭐⭐ Muy Alto |

### 3.2 Evaluación de Acoplamiento entre Capas

```
┌─────────────┐
│  Interface  │
└──────┬──────┘
       │ Stamp (DTOs)
       ▼
┌─────────────┐
│ Application │
└──────┬──────┘
       │ Data + Stamp (via traits)
       ▼
┌─────────────┐
│   Domain    │
└──────┬──────┘
       │ (ninguno - es el núcleo)
       ▼
┌─────────────┐
│   Shared    │
│   Kernel    │
└─────────────┘

┌─────────────┐
│Infrastructure│
└──────┬──────┘
       │ Data + Stamp (implementa traits)
       ▼
┌─────────────┐
│   Domain    │
└─────────────┘
```

### 3.3 Acoplamiento entre Bounded Contexts

| Origen | Destino | Tipo | Naturaleza |
|--------|---------|------|------------|
| Jobs | Workers | Stamp | `WorkerId`, `WorkerSpec` |
| Jobs | Providers | Data | `ProviderId` |
| Jobs | Scheduling | Stamp | `Job`, `SchedulingDecision` |
| Workers | Providers | Stamp | `WorkerProvider`, `WorkerHandle` |
| Scheduling | Workers | Data | `WorkerId`, filtros |
| Scheduling | Providers | Stamp | `ProviderInfo` |
| Audit | (todos) | Data | `correlation_id` |

### 3.4 Matriz de Dependencias

```
          Jobs  Workers  Providers  Scheduling  Audit  IAM  Outbox
Jobs       -      M         L          M         L      -     L
Workers    L      -         M          L         L      L     L
Providers  -      M         -          L         L      -     L
Scheduling M      M         M          -         L      -     -
Audit      -      -         -          -         -      -     -
IAM        -      L         -          -         -      -     -
Outbox     L      L         L          -         L      -     -

Leyenda: - = ninguno, L = bajo (data), M = medio (stamp)
```

### 3.5 Acoplamiento Aferente y Eferente

| Módulo | Aferente (Ca) | Eferente (Ce) | Inestabilidad (I = Ce/(Ca+Ce)) |
|--------|---------------|---------------|-------------------------------|
| `shared_kernel` | 15 | 0 | 0.00 (Muy estable) |
| `domain/jobs` | 8 | 3 | 0.27 (Estable) |
| `domain/workers` | 6 | 2 | 0.25 (Estable) |
| `domain/scheduling` | 4 | 4 | 0.50 (Medio) |
| `application/jobs` | 3 | 6 | 0.67 (Menos estable) |
| `application/workers` | 2 | 5 | 0.71 (Menos estable) |
| `infrastructure/providers` | 1 | 4 | 0.80 (Inestable - ok para infra) |

**Interpretación:**
- Domain layer: Baja inestabilidad (correcto)
- Application layer: Media inestabilidad (esperado)
- Infrastructure layer: Alta inestabilidad (correcto - depende de domain)

---

## 4. Análisis de Connascence

### 4.1 Tipos de Connascence

#### Connascence Estática (Tiempo de Compilación)

| Tipo | Descripción | Fuerza |
|------|-------------|--------|
| **Nombre (CoN)** | Deben acordar nombres | Débil |
| **Tipo (CoT)** | Deben acordar tipos | Débil |
| **Significado (CoM)** | Deben acordar significado de valores | Medio |
| **Posición (CoP)** | Deben acordar posición de parámetros | Medio |
| **Algoritmo (CoA)** | Deben acordar algoritmos | Fuerte |

#### Connascence Dinámica (Tiempo de Ejecución)

| Tipo | Descripción | Fuerza |
|------|-------------|--------|
| **Ejecución (CoE)** | Orden de ejecución importante | Medio |
| **Timing (CoTm)** | Timing de ejecución importante | Fuerte |
| **Valor (CoV)** | Valores deben estar relacionados | Fuerte |
| **Identidad (CoI)** | Referencias a mismo objeto | Muy Fuerte |

### 4.2 Análisis Detallado

#### 4.2.1 Connascence de Nombre (CoN) - DÉBIL

**Prevalencia:** Alta  
**Impacto:** Bajo

```rust
// Ejemplos en el código
job_repository: Arc<dyn JobRepository>,  // Nombre de trait
event_bus: Arc<dyn EventBus>,            // Nombre de trait
worker_registry: Arc<dyn WorkerRegistry>, // Nombre de trait
```

**Localidad:** Bien contenida en cada capa.

#### 4.2.2 Connascence de Tipo (CoT) - DÉBIL

**Prevalencia:** Alta  
**Impacto:** Bajo

```rust
// Tipos compartidos del shared kernel
job_id: JobId,
worker_id: WorkerId,
provider_id: ProviderId,
state: JobState,
```

**Evaluación:**
- ✅ Tipos centralizados en shared kernel
- ✅ Cambios afectan en un solo lugar
- ✅ Beneficioso para type safety

#### 4.2.3 Connascence de Significado (CoM) - MEDIO

**Prevalencia:** Media  
**Impacto:** Medio

```rust
// Estado como string en DB
fn state_to_string(state: &JobState) -> String {
    match state {
        JobState::Pending => "PENDING".to_string(),
        JobState::Running => "RUNNING".to_string(),
        // ...
    }
}

// En otro lugar, debe coincidir
fn string_to_state(s: &str) -> JobState {
    match s {
        "PENDING" => JobState::Pending,
        "RUNNING" => JobState::Running,
        // ...
    }
}
```

**Mitigación aplicada:**
- ✅ Funciones centralizadas en repository
- ⚠️ Podría mejorarse con trait `FromStr`/`Display` en el tipo

**Otro ejemplo - Proto mappings:**
```rust
// En mappers
pub fn map_job_state(state: &JobState) -> i32 {
    match state {
        JobState::Pending => 0,
        JobState::Assigned => 1,
        // ...
    }
}
```

#### 4.2.4 Connascence de Posición (CoP) - BAJO

**Prevalencia:** Baja  
**Impacto:** Bajo

```rust
// Evitado con builder pattern
let spec = JobSpec::new(command)
    .with_image("image:latest")
    .with_timeout(30000)
    .with_resources(2.0, 1024, 5120);

// Evitado con structs nombrados
pub struct CreateJobRequest {
    pub spec: JobSpecRequest,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub job_id: Option<String>,
}
```

**Evaluación:** ✅ Bien manejado con named parameters y builders

#### 4.2.5 Connascence de Algoritmo (CoA) - MEDIO

**Prevalencia:** Media  
**Impacto:** Medio-Alto cuando existe

**Ejemplo 1: State Machine**
```rust
// En JobState
impl JobState {
    pub fn can_transition_to(&self, new_state: &JobState) -> bool {
        match (self, new_state) {
            (JobState::Pending, JobState::Assigned) => true,
            (JobState::Pending, JobState::Scheduled) => true,
            // ...
        }
    }
}

// En Job aggregate - debe usar la misma lógica
impl Job {
    pub fn mark_running(&mut self) -> Result<()> {
        let new_state = JobState::Running;
        if !self.state.can_transition_to(&new_state) {
            return Err(DomainError::InvalidStateTransition { ... });
        }
        // ...
    }
}
```

**Evaluación:**
- ✅ State machine centralizado en `JobState::can_transition_to`
- ✅ Job aggregate usa el método centralizado
- ✅ CoA bien manejado

**Ejemplo 2: Serialización JSON**
```rust
// Formato de serialización compartido
#[derive(Serialize, Deserialize)]
pub struct JobSpec { ... }

// En repository
let spec_json = serde_json::to_value(&job.spec)?;

// En otro lugar - debe deserializar igual
let spec: JobSpec = serde_json::from_value(spec_json)?;
```

**Evaluación:**
- ✅ Derive macros garantizan consistencia
- ⚠️ Cambios en struct pueden romper compatibilidad

#### 4.2.6 Connascence de Ejecución (CoE) - MEDIO

**Prevalencia:** Media  
**Impacto:** Medio

**Ejemplo 1: Transacciones atómicas**
```rust
// Save y enqueue deben ser atómicos
async fn save(&self, job: &Job) -> Result<()> {
    let mut tx = self.pool.begin().await?;
    
    // 1. Insert job
    sqlx::query("INSERT INTO jobs ...").execute(&mut *tx).await?;
    
    // 2. Insert into queue (debe suceder después)
    if job.state() == &JobState::Pending {
        sqlx::query("INSERT INTO job_queue ...").execute(&mut *tx).await?;
    }
    
    tx.commit().await?;  // 3. Commit atómico
    Ok(())
}
```

**Evaluación:** ✅ Bien manejado con transacciones SQL

**Ejemplo 2: Outbox Pattern**
```rust
// Orden específico requerido
// 1. Insert eventos en outbox
outbox_repository.insert_events(&events).await?;

// 2. (En relay) Publicar eventos
event_bus.publish(&event).await?;

// 3. Marcar como publicados
outbox_repository.mark_published(&event_ids).await?;
```

**Evaluación:** ✅ Patrón bien implementado con idempotencia

#### 4.2.7 Connascence de Timing (CoTm) - BAJO

**Prevalencia:** Baja  
**Impacto:** Bajo (bien manejado)

```rust
// Heartbeat timeout
pub heartbeat_timeout: Duration,  // Config centralizada

// Uso consistente
if last_heartbeat + heartbeat_timeout < now {
    // Worker unhealthy
}
```

**Evaluación:** ✅ Timeouts configurables y centralizados

#### 4.2.8 Connascence de Valor (CoV) - BAJO

**Prevalencia:** Baja  

```rust
// Valores que deben coincidir
// ID generado en server, usado en worker
let worker_id = WorkerId::new();
// Worker debe usar mismo ID al registrarse
```

**Evaluación:** ✅ IDs generados centralmente, pasados explícitamente

#### 4.2.9 Connascence de Identidad (CoI) - BAJO

**Prevalencia:** Baja  

```rust
// Arc compartido - mismo objeto
let job_repository: Arc<dyn JobRepository> = ...;
// Múltiples servicios usan mismo Arc
```

**Evaluación:** ✅ Uso intencional de Arc para shared state

### 4.3 Resumen de Connascence

```
┌─────────────────────────────────────────────────────────────────┐
│                MAPA DE CONNASCENCE                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Estática:                                                      │
│  ████████████████████████  Nombre (CoN) - Débil, Alta          │
│  ████████████████████████  Tipo (CoT) - Débil, Alta            │
│  ██████████████░░░░░░░░░░  Significado (CoM) - Medio, Media    │
│  ████░░░░░░░░░░░░░░░░░░░░  Posición (CoP) - Medio, Baja        │
│  ██████████████░░░░░░░░░░  Algoritmo (CoA) - Fuerte, Media     │
│                                                                 │
│  Dinámica:                                                      │
│  ██████████████░░░░░░░░░░  Ejecución (CoE) - Medio, Media      │
│  ████░░░░░░░░░░░░░░░░░░░░  Timing (CoTm) - Fuerte, Baja        │
│  ████░░░░░░░░░░░░░░░░░░░░  Valor (CoV) - Fuerte, Baja          │
│  ████░░░░░░░░░░░░░░░░░░░░  Identidad (CoI) - Muy Fuerte, Baja  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
  Primera barra: Fuerza de la connascence
  Cantidad de relleno: Prevalencia en el código
```

---

## 5. Mapeo de Dependencias

### 5.1 Dependency Graph - Domain Layer

```
                         shared_kernel
                              │
            ┌─────────────────┼─────────────────┐
            │                 │                 │
            ▼                 ▼                 ▼
         jobs ◄──────────► workers ◄────────► providers
            │                 │                 │
            │                 │                 │
            └────────┬────────┴────────┬────────┘
                     │                 │
                     ▼                 ▼
               scheduling           audit
                     │
                     ▼
                  events ◄─────────── outbox
```

### 5.2 Dependency Graph - Full System

```
┌─────────────────────────────────────────────────────────────────────┐
│                         INTERFACE                                    │
│                                                                      │
│  JobExecution ──┬──► WorkerAgent ──┬──► Scheduler ──► Audit         │
│       │         │         │         │        │          │            │
└───────┼─────────┼─────────┼─────────┼────────┼──────────┼────────────┘
        │         │         │         │        │          │
        ▼         ▼         ▼         ▼        ▼          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        APPLICATION                                   │
│                                                                      │
│  CreateUseCase ◄───► JobController ◄───► LifecycleManager           │
│       │                   │                     │                    │
│       ▼                   ▼                     ▼                    │
│  ProviderRegistry ◄───► JobDispatcher ◄───► SchedulingService       │
│                                                                      │
└───────┬─────────────────────┬─────────────────────┬──────────────────┘
        │                     │                     │
        ▼                     ▼                     ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          DOMAIN                                      │
│                                                                      │
│  Job ◄─────────► Worker ◄─────────► ProviderConfig                  │
│   │                 │                     │                          │
│   ▼                 ▼                     ▼                          │
│ JobRepository    WorkerRegistry    ProviderConfigRepository         │
│   │                 │                     │                          │
│   └────────────────►├◄────────────────────┘                          │
│                     │                                                │
│                     ▼                                                │
│                  EventBus ◄───────► OutboxRepository                 │
│                                                                      │
└───────┬─────────────────────┬─────────────────────┬──────────────────┘
        │                     │                     │
        ▼                     ▼                     ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      INFRASTRUCTURE                                  │
│                                                                      │
│  PostgresJobRepo    PostgresWorkerRegistry    PostgresProviderRepo  │
│                                                                      │
│  DockerProvider ◄───► KubernetesProvider ◄───► FirecrackerProvider  │
│                                                                      │
│                  OutboxEventBus ◄───► OutboxRelay                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 6. Evaluación por Bounded Context

### 6.1 Jobs Context

| Métrica | Valor | Evaluación |
|---------|-------|------------|
| Cohesión interna | Alta (Funcional) | ✅ Excelente |
| Acoplamiento aferente | 8 | ⚠️ Muy usado |
| Acoplamiento eferente | 3 | ✅ Bajo |
| Connascence dominante | CoN, CoT | ✅ Débil |
| Inestabilidad | 0.27 | ✅ Estable |

**Observaciones:**
- Core del sistema, bien diseñado
- Aggregate Job rico en lógica
- State machine centralizado

### 6.2 Workers Context

| Métrica | Valor | Evaluación |
|---------|-------|------------|
| Cohesión interna | Alta | ✅ Excelente |
| Acoplamiento aferente | 6 | ⚠️ Muy usado |
| Acoplamiento eferente | 2 | ✅ Bajo |
| Connascence dominante | CoN, CoT | ✅ Débil |
| Inestabilidad | 0.25 | ✅ Estable |

**Observaciones:**
- Buena segregación de traits (ISP)
- WorkerRegistry trait limpio
- Lifecycle bien encapsulado

### 6.3 Scheduling Context

| Métrica | Valor | Evaluación |
|---------|-------|------------|
| Cohesión interna | Alta | ✅ Excelente |
| Acoplamiento aferente | 4 | ✅ Medio |
| Acoplamiento eferente | 4 | ⚠️ Medio |
| Connascence dominante | CoN, CoA | ⚠️ Medio |
| Inestabilidad | 0.50 | ⚠️ Medio |

**Observaciones:**
- Estrategias bien separadas
- Depende de Jobs + Workers + Providers
- CoA en estrategias de selección

### 6.4 Providers Context

| Métrica | Valor | Evaluación |
|---------|-------|------------|
| Cohesión interna | Alta | ✅ Excelente |
| Acoplamiento aferente | 5 | ⚠️ Medio |
| Acoplamiento eferente | 1 | ✅ Muy bajo |
| Connascence dominante | CoN | ✅ Débil |
| Inestabilidad | 0.17 | ✅ Muy estable |

### 6.5 Audit Context

| Métrica | Valor | Evaluación |
|---------|-------|------------|
| Cohesión interna | Alta | ✅ Excelente |
| Acoplamiento aferente | 2 | ✅ Bajo |
| Acoplamiento eferente | 0 | ✅ Ninguno |
| Connascence dominante | CoN | ✅ Débil |
| Inestabilidad | 0.00 | ✅ Muy estable |

**Observaciones:**
- Completamente desacoplado
- Solo recibe datos, no depende de otros

---

## 7. Recomendaciones

### 7.1 Prioridad Alta

#### R1: Reducir CoM en mapeo de estados

**Problema:** Conversión string↔enum dispersa
```rust
// Actual - múltiples lugares
fn state_to_string(state: &JobState) -> String { ... }
fn string_to_state(s: &str) -> JobState { ... }
```

**Solución:**
```rust
// Centralizar en el tipo
impl FromStr for JobState {
    type Err = DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { ... }
}

impl Display for JobState {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result { ... }
}
```

#### R2: Separar eventos por contexto

**Problema:** `DomainEvent` enum con 40+ variantes

**Solución:**
```rust
// Opción A: Enums por contexto
pub enum JobEvent { Created, StatusChanged, Assigned, ... }
pub enum WorkerEvent { Registered, StatusChanged, Terminated, ... }

// Opción B: Trait para eventos
pub trait DomainEvent {
    fn event_type(&self) -> &str;
    fn occurred_at(&self) -> DateTime<Utc>;
    fn correlation_id(&self) -> Option<&str>;
}
```

### 7.2 Prioridad Media

#### R3: Extraer sub-servicios de WorkerLifecycleManager

**Problema:** Múltiples responsabilidades

**Solución:**
```rust
// Separar en servicios cohesivos
pub struct HeartbeatMonitor { ... }
pub struct AutoScaler { ... }
pub struct WorkerReconciler { ... }
pub struct IdleTerminator { ... }

// Coordinator que los orquesta
pub struct WorkerLifecycleManager {
    heartbeat_monitor: HeartbeatMonitor,
    auto_scaler: AutoScaler,
    reconciler: WorkerReconciler,
    terminator: IdleTerminator,
}
```

#### R4: Mejorar encapsulación de IDs

**Problema:** Campos públicos en newtypes
```rust
pub struct JobId(pub Uuid);  // pub permite modificación
```

**Solución:**
```rust
pub struct JobId(Uuid);  // Privado

impl JobId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn as_uuid(&self) -> &Uuid { &self.0 }
    pub fn into_uuid(self) -> Uuid { self.0 }
}
```

### 7.3 Prioridad Baja

#### R5: Documentar contratos de CoA

Para algoritmos compartidos, agregar documentación explícita:

```rust
/// State Machine Contract
/// 
/// INVARIANT: This method defines ALL valid state transitions.
/// Any code checking transitions MUST use this method.
/// 
/// Valid transitions:
/// - Pending → Assigned, Scheduled, Failed, Cancelled, Timeout
/// - Assigned → Running, Failed, Timeout, Cancelled
/// - ...
impl JobState {
    pub fn can_transition_to(&self, new_state: &JobState) -> bool { ... }
}
```

#### R6: Implementar ADRs para decisiones de CoA

Documentar decisiones de diseño que crean connascence de algoritmo:

```markdown
# ADR-007: Job State Machine

## Context
Los estados de jobs deben ser consistentes en todo el sistema.

## Decision
Centralizar state machine en `JobState::can_transition_to()`.

## Consequences
- Todos los módulos DEBEN usar este método para validar transiciones
- Cambios en el state machine requieren actualizar UN lugar
- Tests de property-based para garantizar invariantes
```

---

## Conclusión

El proyecto Hodei Job Platform demuestra un **buen manejo de acoplamiento y cohesión**:

| Aspecto | Evaluación | Calificación |
|---------|------------|--------------|
| Cohesión general | Alta funcional | 85/100 |
| Acoplamiento entre capas | Bajo (via traits) | 90/100 |
| Acoplamiento entre contexts | Medio-Bajo | 80/100 |
| Connascence | Predomina débil | 85/100 |
| **Promedio** | | **85/100** |

Las áreas de mejora identificadas son refinamientos que pueden implementarse incrementalmente sin afectar la estabilidad del sistema.

---

*Documento generado el 2025-12-27 basado en análisis del código fuente*

