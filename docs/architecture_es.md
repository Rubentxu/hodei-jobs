# Documentación de Arquitectura - Plataforma Hodei Jobs

**Versión**: 8.0  
**Última Actualización**: 2026-01-09  
**Estilo Arquitectónico**: Hexagonal + Event-Driven + DDD  

---

## Tabla de Contenidos

1. [Visión General](#visión-general)
2. [Principios Arquitectónicos](#principios-arquitectónicos)
3. [Arquitectura de Alto Nivel](#arquitectura-de-alto-nivel)
4. [Estructura del Workspace](#estructura-del-workspace)
5. [Capa de Dominio (Servidor)](#capa-de-dominio-servidor)
6. [Capa de Aplicación (Servidor)](#capa-de-aplicación-servidor)
7. [Capa de Infraestructura (Servidor)](#capa-de-infraestructura-servidor)
8. [Capa de Interfaz (Servidor)](#capa-de-interfaz-servidor)
9. [Arquitectura del Worker](#arquitectura-del-worker)
10. [Contextos Delimitados (Bounded Contexts)](#contextos-delimitados-bounded-contexts)
11. [Arquitectura de Eventos de Dominio](#arquitectura-de-eventos-de-dominio)
12. [Flujo de Ejecución de Trabajos](#flujo-de-ejecución-de-trabajos)
13. [Gestión del Ciclo de Vida del Worker](#gestión-del-ciclo-de-vida-del-worker)
14. [Proveedores de Workers](#proveedores-de-workers)
15. [Servicios gRPC](#servicios-grpc)
16. [Mensajería y Bus de Eventos](#mensajería-y-bus-de-eventos)
17. [Persistencia](#persistencia)
18. [Seguridad (Zero Trust)](#seguridad-zero-trust)
19. [Optimizaciones de Rendimiento (v8.0)](#optimizaciones-de-rendimiento-v80)
20. [Estrategia de Pruebas](#estrategia-de-pruebas)
21. [Gestión de Configuración](#gestión-de-configuración)
22. [Arquitectura de Despliegue](#arquitectura-de-despliegue)

---

## Visión General

La Plataforma Hodei Jobs es una **plataforma de ejecución de trabajos distribuida y lista para producción y HPC**, construida con Rust. Proporciona una abstracción universal para ejecutar cargas de trabajo computacionales a través de múltiples proveedores de infraestructura (Docker, Kubernetes, Firecracker) con aprovisionamiento automático de workers y seguridad Zero Trust.

### Características Clave

- **Arquitectura Orientada a Eventos**: Reacciona a eventos de dominio en tiempo real
- **Arquitectura Hexagonal**: Clara separación entre lógica de dominio e infraestructura
- **Diseño Guiado por el Dominio (DDD)**: Organizado alrededor de contextos delimitados
- **Patrón CQRS**: Responsabilidades separadas para comandos y consultas
- **Patrón Saga**: Coordinación de transacciones distribuidas
- **Seguridad Zero Trust**: mTLS con infraestructura PKI
- **Listo para HPC**: Optimizado para cargas de trabajo de computación de alto rendimiento

### Stack Tecnológico

- **Lenguaje**: Rust Edición 2024 (1.83+)
- **Runtime**: Tokio async runtime
- **Comunicación**: gRPC (Tonic) + Protocol Buffers
- **Base de Datos**: PostgreSQL con SQLx
- **Mensajería**: PostgreSQL Outbox + NATS (opcional)
- **Proveedores**: Docker (Bollard), Kubernetes (kube-rs), Firecracker (KVM)

---

## Principios Arquitectónicos

### 1. Comunicación Inside-Out
**Principio**: Todos los agentes inician conexiones hacia el plano de control. Nunca abrir puertos de entrada en los workers.

```
Worker ────OUTBOUND────▶ Servidor (Plano de Control)
   ▲                           │
   │                           │
   └──────────gRPC─────────────┘
```

### 2. Diseño Guiado por el Dominio (DDD)
- **Contextos Delimitados**: Cada área de dominio tiene límites claros
- **Lenguaje Ubicuo**: Terminología consistente a través de las capas
- **Raíces de Agregado**: Aseguran consistencia dentro de los agregados
- **Eventos de Dominio**: Capturan cambios importantes del dominio

### 3. Arquitectura Hexagonal
```
┌─────────────────────────────────────────────────┐
│              Capa de Presentación                │
│         (gRPC, REST, CLI, Agente Worker)         │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│            Capa de Aplicación                    │
│     (Casos de Uso, Sagas, Coordinadores)         │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│              Capa de Dominio                     │
│   (Agregados, Objetos de Valor, Eventos)         │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│          Capa de Infraestructura                 │
│  (Repositorios, Proveedores, Bus de Eventos)     │
└──────────────────────────────────────────────────┘
```

### 4. Principios SOLID
- **Responsabilidad Única**: Cada componente tiene una razón para cambiar
- **Abierto/Cerrado**: Abierto para extensión, cerrado para modificación
- **Sustitución de Liskov**: Los subtipos deben ser sustituibles
- **Segregación de Interfaz**: Los clientes no deben depender de interfaces no usadas
- **Inversión de Dependencia**: Depender de abstracciones, no de concreciones

### 5. Reactivo y Orientado a Eventos
- **Streams Reactivos**: Operaciones asíncronas conscientes de la contrapresión (backpressure)
- **Eventos de Dominio**: Reaccionar a cambios de estado asíncronamente
- **Patrón Saga**: Coordinar transacciones distribuidas
- **Modelo de Actores**: Gestión aislada del estado del worker

---

## Arquitectura de Alto Nivel

### Visión General de Componentes

```mermaid
graph TB
    subgraph "Clientes"
        CLI[CLI]
        WEB[Dashboard Web]
        API[Clientes REST/gRPC]
    end

    subgraph "Plano de Control (Servidor)"
        subgraph "Capa de Interfaz"
            GRPC[Servicios gRPC]
            REST[API REST]
            WEBSOCKET[Streams WebSocket]
        end

        subgraph "Capa de Aplicación"
            JOB_UC[Casos de Uso Jobs]
            WORKER_UC[Casos de Uso Workers]
            PROV_UC[Casos de Uso Proveedores]
            SCHED_UC[Planificador]
            SAGA[Orquestador Saga]
        end

        subgraph "Capa de Dominio"
            JOB_CTX[Contexto Jobs]
            WORKER_CTX[Contexto Workers]
            PROV_CTX[Contexto Proveedores]
            IAM_CTX[Contexto IAM]
        end
    end

    subgraph "Plano de Datos (Workers)"
        WORKER[Agente Worker]
        EXEC[Ejecutor de Trabajos]
        MONITOR[Monitor de Recursos]
    end

    subgraph "Infraestructura"
        PG[(PostgreSQL)]
        NATS[Broker NATS]
        DOCKER[Daemon Docker]
        K8S[API Kubernetes]
        FIRECRACKER[Firecracker]
    end

    CLI --> GRPC
    WEB --> REST
    API --> GRPC

    GRPC --> JOB_UC
    REST --> JOB_UC
    GRPC --> WORKER_UC

    JOB_UC --> JOB_CTX
    WORKER_UC --> WORKER_CTX
    PROV_UC --> PROV_CTX
    SCHED_UC --> JOB_CTX

    SAGA --> JOB_UC
    SAGA --> WORKER_UC

    JOB_UC --> PG
    WORKER_UC --> PG
    SAGA --> PG

    WORKER --> GRPC
    WORKER --> EXEC
    EXEC --> MONITOR

    WORKER_UC --> DOCKER
    WORKER_UC --> K8S
    WORKER_UC --> FIRECRACKER

    JOB_UC --> NATS
    WORKER_UC --> NATS
```

### Flujos de Comunicación

#### 1. Flujo de Envío de Trabajo
```
Cliente ──QueueJob──▶ Servidor gRPC ──CreateJobUseCase──▶ Dominio
                          │                              │
                          ▼                              ▼
                    JobQueue ◀──────────JobCreatedEvent
                          │
                          ▼
                    AprovisionamientoWorker
                          │
                          ▼
                    Proveedor (Docker/K8s/FC)
                          │
                          ▼
                    Agente Worker ◀─────────WorkerStartedEvent
                          │
                          ▼
                    EjecuciónTrabajo
                          │
                          ▼
                    JobCompletedEvent
```

#### 2. Flujo de Registro de Worker (Autenticación OTP)
```
Proveedor ──create_worker──▶ Servidor ──GenerarOTP──▶ TokenStore
    │                                             │
    ▼                                             ▼
Contenedor (con OTP) ────────────────────────────▶ Servidor
         │
         ▼
    Agente Worker
         │
         ▼
    Registrar(OTP) ──▶ Servidor (ValidarOTP) ──▶ WorkerRegisteredEvent
```

---

## Estructura del Workspace

### Organización de Crates

```
hodei-jobs/
├── crates/
│   ├── shared/                          # Tipos y utilidades compartidas
│   │   ├── src/
│   │   │   ├── ids.rs                   # JobId, WorkerId, ProviderId, etc.
│   │   │   ├── states.rs                # JobState, WorkerState, ProviderStatus
│   │   │   ├── error.rs                 # Tipos de error comunes
│   │   │   └── event_topics.rs          # Definiciones de tópicos de eventos
│   │
│   ├── server/                          # Componentes del lado del servidor
│   │   ├── domain/                      # Capa de Dominio (DDD)
│   │   │   ├── src/
│   │   │   │   ├── shared_kernel/        # Conceptos de dominio compartidos
│   │   │   │   ├── jobs/                # Contexto delimitado de Jobs
│   │   │   │   ├── workers/             # Contexto delimitado de Workers
│   │   │   │   ├── providers/           # Contexto delimitado de Proveedores
│   │   │   │   ├── scheduling/          # Estrategias de planificación
│   │   │   │   ├── templates/           # Plantillas de trabajos (EPIC-34)
│   │   │   │   ├── iam/                 # Gestión de Identidad y Acceso
│   │   │   │   ├── audit/               # Registro de auditoría
│   │   │   │   ├── credentials/         # Gestión de credenciales
│   │   │   │   ├── saga/                # Soporte para patrón Saga
│   │   │   │   ├── logging/             # Dominio de logging
│   │   │   │   ├── command/             # Soporte para bus de comandos
│   │   │   │   ├── domain_events/       # Eventos de dominio modulares
│   │   │   │   ├── event_bus.rs         # Traits del bus de eventos
│   │   │   │   ├── outbox/              # Patrón Outbox
│   │   │   │   ├── telemetry/           # Trazas y métricas
│   │   │   │   └── events.rs            # Eventos monolíticos heredados
│   │   │
│   │   ├── application/                 # Capa de Aplicación (Casos de Uso)
│   │   │   ├── src/
│   │   │   │   ├── core/                # Infraestructura CQRS
│   │   │   │   ├── command/             # Implementación del bus de comandos
│   │   │   │   ├── jobs/                # Casos de uso de Jobs
│   │   │   │   ├── workers/             # Casos de uso de Workers
│   │   │   │   ├── providers/           # Casos de uso de Proveedores
│   │   │   │   ├── scheduling/          # Servicio de planificación
│   │   │   │   ├── saga/                # Orquestadores Saga
│   │   │   │   ├── resilience/          # Circuit breakers, reintentos
│   │   │   │   └── metrics/             # Recolección de métricas
│   │   │
│   │   ├── infrastructure/              # Capa de Infraestructura
│   │   │   ├── src/
│   │   │   │   ├── persistence/         # Implementaciones de base de datos
│   │   │   │   │   └── postgres/        # Repositorios PostgreSQL
│   │   │   │   ├── providers/           # Implementaciones de proveedores
│   │   │   │   │   ├── docker.rs        # Proveedor Docker
│   │   │   │   │   ├── kubernetes/      # Proveedor Kubernetes
│   │   │   │   │   └── firecracker/     # Proveedor Firecracker
│   │   │   │   ├── messaging/           # Implementaciones del bus de eventos
│   │   │   │   │   ├── postgres/        # Outbox PostgreSQL
│   │   │   │   │   └── nats/            # Bus de eventos NATS
│   │   │   │   ├── observability/       # Logging y monitoreo
│   │   │   │   ├── metrics/             # Recolectores de métricas
│   │   │   │   └── reconciliation/      # Reconciliación de estado
│   │   │   └── migrations/              # Migraciones de base de datos
│   │   │
│   │   ├── interface/                   # Capa de Interfaz (Adaptadores)
│   │   │   ├── src/
│   │   │   │   ├── grpc/                # Implementaciones de servicios gRPC
│   │   │   │   ├── mappers/             # Mappers gRPC ↔ Dominio
│   │   │   │   ├── log_buffer.rs        # Buffer de streaming de logs
│   │   │   │   └── log_persistence.rs   # Backend de almacenamiento de logs
│   │   │
│   │   ├── bin/                         # Ejecutables del servidor
│   │   │   └── src/
│   │   │       └── main.rs              # Punto de entrada del servidor gRPC
│   │   │
│   │   └── tests/                       # Pruebas de integración
│   │
│   ├── worker/                          # Componentes del lado del worker
│   │   ├── domain/                      # Dominio del worker (mínimo)
│   │   ├── application/                 # Capa de aplicación del worker
│   │   ├── infrastructure/              # Infraestructura del worker
│   │   │   └── src/
│   │   │       ├── executor.rs          # Ejecutor de trabajos
│   │   │       ├── metrics.rs           # Recolector de métricas
│   │   │       ├── log_batcher.rs       # Batching de logs
│   │   │       └── client.rs            # Cliente gRPC
│   │   └── bin/                         # Ejecutables del worker
│   │       └── src/
│   │           └── main.rs              # Punto de entrada del agente worker
│   │
│   └── cli/                             # Interfaz de línea de comandos
│       └── src/
│
├── proto/                               # Definiciones de Protocol Buffers
│   ├── hodei_all_in_one.proto          # Todos los servicios combinados
│   ├── worker_agent.proto              # API Worker ↔ Servidor
│   ├── job_execution.proto             # API de ejecución de trabajos
│   ├── scheduler.proto                 # API de planificación
│   ├── metrics.proto                   # API de métricas
│   ├── job_templates.proto             # API de plantillas
│   └── provider_management.proto       # API de gestión de proveedores
│
└── docs/                                # Documentación
    ├── architecture.md                  # Este archivo (Inglés)
    ├── architecture_es.md               # Este archivo (Español)
    ├── analysis/                        # Análisis arquitectónico
    └── epics/                           # Épicas de refactorización
```

---

## Capa de Dominio (Servidor)

### Propósito

La capa de dominio contiene la **lógica de negocio y conceptos centrales** de la plataforma. Es completamente independiente de preocupaciones externas (bases de datos, frameworks, APIs).

### Contextos Delimitados (Bounded Contexts)

#### 1. Contexto de Jobs (`domain/src/jobs/`)

**Responsabilidad**: Gestión del ciclo de vida del trabajo

**Componentes Clave**:

**Raíz de Agregado: Job**

**📋 Responsabilidad (SOLID)**:
- **Principio de Responsabilidad Única (SRP)**: El agregado `Job` es responsable de mantener la consistencia del ciclo de vida de un job. Es el único lugar donde se pueden modificar las invariantes de negocio relacionadas con un job.
- **Principio Abierto/Cerrado (OCP)**: Abierto para extensión (nuevos estados, nuevas validaciones) pero cerrado para modificación (métodos públicos estables).

**🏗️ Ámbito en DDD**: **Bounded Context: Jobs**
- Es el **Aggregate Root** del contexto de Jobs, lo que significa que:
- Todas las modificaciones a un Job y sus entidades relacionadas deben pasar por este agregado
- Garantiza la consistencia transaccional dentro del agregado
- Implementa las reglas de negocio del dominio

**🔗 Dependencias**:
- **Internas**: `JobId`, `JobSpec`, `JobState`, `ExecutionContext` (Objetos de Valor del mismo contexto)
- **Sin dependencias externas**: Puro dominio, sin dependencias de frameworks o infraestructura

**💡 Motivación**:
Se creó para encapsular la lógica de negocio completa de un job y garantizar que:
1. Los jobs siempre transicionan de estado de manera válida
2. Las invariantes de negocio (ej. max_attempts, timeout) se respeten
3. Las operaciones atómicas en el agregado sean consistentes

**🎨 Patrones Aplicados**:
- **Patrón Agregado**: Agrupa entidades relacionadas bajo un root para garantizar consistencia
- **Encapsulamiento**: Los campos son privados, se acceden solo a través de métodos públicos
- **Cumplimiento de Invariantes**: Los métodos públicos aseguran que el estado siempre sea válido

```rust
// Raíz de Agregado
pub struct Job {
    id: JobId,
    spec: JobSpec,
    state: JobState,
    selected_provider: Option<ProviderId>,
    execution_context: ExecutionContext,
    attempts: u32,
    max_attempts: u32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

---

**Objetos de Valor: JobSpec y relacionados**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada Objeto de Valor tiene una única responsabilidad: representar un concepto de dominio inmutable y validado.
- **Principio de Sustitución de Liskov (LSP)**: Los Objetos de Valor son sustituibles por igualdad (si tienen mismos valores, son el mismo objeto).

**🏗️ Ámbito en DDD**: **Bounded Context: Jobs**
- Son componentes internos del agregado Job
- No tienen identidad propia, se identifican por sus atributos
- Son inmutables para evitar efectos secundarios

**🔗 Dependencias**:
- `JobSpec`: Depende de `CommandType`, `JobResources`, `JobPreferences`
- `CommandType`: Enum de tipos de comandos soportados
- `JobResources`: Representa requerimientos de recursos computacionales

**💡 Motivación**:
Se crearon para:
1. Capturar conceptos de dominio complejos en objetos cohesivos
2. Evitar **Obsesión por Primitivos** (usar strings, ints en lugar de objetos de dominio)
3. Garantizar inmutabilidad de configuraciones críticas
4. Facilitar validaciones en un solo lugar

**🎨 Patrones Aplicados**:
- **Patrón Value Object**: Objetos sin identidad, definidos por sus atributos
- **Inmutabilidad**: Los objetos no pueden modificarse después de creación
- **Seguridad de Tipos**: Enums en lugar de strings para tipos de comandos

```rust
// Objetos de Valor
pub struct JobSpec {
    command: CommandType,
    image: Option<String>,
    environment: HashMap<String, String>,
    resources: JobResources,
    timeout_ms: u64,
    constraints: Vec<Constraint>,
    inputs: Vec<ArtifactSource>,
    outputs: Vec<ArtifactDest>,
    preferences: JobPreferences,
}

pub enum CommandType {
    Shell { cmd: String, args: Vec<String> },
    Script { interpreter: String, content: String },
}

pub struct JobResources {
    cpu_cores: f32,
    memory_mb: u64,
    storage_mb: u64,
    gpu_required: bool,
    architecture: String,
}
```

---

**Traits de Repositorio: JobRepository y JobQueue**

**📋 Responsabilidad (SOLID)**:
- **Principio de Segregación de Interfaz (ISP)**: Cada trait tiene métodos cohesivos. `JobRepository` para persistencia, `JobQueue` para operaciones de cola. Los clientes no dependen de métodos que no usan.
- **Principio de Inversión de Dependencia (DIP)**: Las capas superiores dependen de abstracciones (traits), no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Implementación de Infraestructura de Abstracciones de Dominio**
- Son **puertos** de la arquitectura hexagonal
- Definen contratos que debe cumplir cualquier implementación de infraestructura
- Permiten cambiar implementaciones (Postgres, MySQL, en memoria) sin afectar el dominio

**🔗 Dependencias**:
- **Tipos de Dominio**: `Job`, `JobId`, `JobState`, `Result<Job>`
- **Traits Asíncronos**: `#[async_trait]` para métodos asíncronos
- **Concurrencia**: `Send + Sync` para permitir concurrencia segura

**💡 Motivación**:
Se crearon para:
1. Separar la lógica de negocio (dominio) de la persistencia (infraestructura)
2. Permitir múltiples implementaciones (Postgres, en memoria, caching)
3. Facilitar testing con mocks y fakes
4. Seguir el patrón Repository para encapsular acceso a datos

**🎨 Patrones Aplicados**:
- **Patrón Repository**: Abstrae el almacenamiento y recuperación de objetos de dominio
- **Inversión de Dependencia**: Depende de abstracciones, no de implementaciones
- **Segregación de Interfaz**: Interfaces pequeñas y enfocadas

```rust
// Traits de Repositorio
#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn save(&self, job: &Job) -> Result<()>;
    async fn find_by_id(&self, id: &JobId) -> Result<Option<Job>>;
    async fn find_by_state(&self, state: &JobState) -> Result<Vec<Job>>;
    async fn find_pending(&self) -> Result<Vec<Job>>;
    async fn delete(&self, id: &JobId) -> Result<()>;
    // ... más métodos
}

#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: Job) -> Result<()>;
    async fn dequeue(&self) -> Result<Option<Job>>;
    async fn peek(&self) -> Result<Option<Job>>;
    async fn len(&self) -> Result<usize>;
    // ... más métodos
}
```

**Eventos de Dominio: JobEvent**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada variante del enum representa un evento específico que ocurrió en el dominio. El enum como un todo es responsable de representar todos los eventos posibles de jobs.
- **OCP**: Nuevos eventos pueden agregarse sin modificar el código existente que maneja eventos (mediante pattern matching exhaustivo).

**🏗️ Ámbito en DDD**: **Bounded Context: Jobs - Bus de Eventos**
- Representa **algo que pasó** en el dominio que es relevante para otras partes del sistema
- Son inmutables por naturaleza (los hechos del pasado no cambian)
- Publicados por el agregado cuando ocurren cambios de estado importantes

**🔗 Dependencias**:
- **Tipos de Dominio**: `JobId`, `JobSpec`, `JobState`, `JobExecutionResult`
- **Serialización**: `#[derive(Serialize, Deserialize)]` para serialización JSON
- **Sin dependencias externas**: Solo tipos de dominio

**💡 Motivación**:
Se creó para:
1. **Desacoplar contextos delimitados**: Los cambios en Jobs no impactan directamente a Workers, Scheduling, etc.
2. **Implementar Arquitectura Orientada a Eventos**: Permitir reacción asíncrona a cambios
3. **Auditoría y seguimiento**: Historia completa de lo que pasó con cada job
4. **Integración con sistemas externos**: Notificaciones, logs, métricas

**🎨 Patrones Aplicados**:
- **Patrón Eventos de Dominio**: Captura hechos importantes del dominio
- **Event Sourcing (parcial)**: Eventos representan cambios de estado
- **Pub/Sub**: Productores (agregados) publican, consumidores suscriben
- **Seguridad de Tipos**: Enum asegura que solo se manejen eventos válidos

**⚠️ Consideraciones de Diseño**:
- **Connascence of Name**: El nombre del evento debe ser claro y descriptivo
- **Acoplamiento Temporal**: Los consumidores deben manejar el orden correcto de eventos
- **Idempotencia**: Los manejadores de eventos deben ser idempotentes

```rust
pub enum JobEvent {
    JobCreated {
        job_id: JobId,
        spec: JobSpec,
        queued_by: String,
    },
    JobStatusChanged {
        job_id: JobId,
        old_state: JobState,
        new_state: JobState,
    },
    JobCompleted {
        job_id: JobId,
        result: JobExecutionResult,
    },
    JobFailed {
        job_id: JobId,
        error: String,
        attempt: u32,
    },
    // ... más eventos
}
```

#### 2. Contexto de Workers (`domain/src/workers/`)

**Responsabilidad**: Gestión del ciclo de vida del worker

**🏗️ Ámbito en DDD**: **Bounded Context: Workers**
- Maneja el ciclo de vida completo de workers (aprovisionamiento, registro, ejecución, terminación)
- Define los contratos para proveedores de infraestructura
- Coordina con el contexto de Jobs para asignación de trabajos

**📋 Responsabilidad General (SOLID)**:
- **SRP**: El contexto está dedicado exclusivamente a la gestión de workers
- **ISP**: Cada trait tiene métodos cohesivos específicos para su propósito
- **DIP**: Las capas superiores dependen de abstracciones, no de implementaciones concretas

**Componentes Clave**:

---

**Raíz de Agregado: Worker**

**📋 Responsabilidad (SOLID)**:
- **SRP**: El agregado `Worker` es responsable de mantener la consistencia del estado de un worker durante todo su ciclo de vida.
- **OCP**: Abierto para agregar nuevos estados o políticas de TTL sin modificar código existente.

**🏗️ Ámbito en DDD**: **Raíz de Agregado - Contexto Workers**
- Es el punto de consistencia para todas las operaciones relacionadas con un worker
- Garantiza que el estado del worker y sus recursos sean consistentes
- Coordina transiciones de estado validadas

**🔗 Dependencias**:
- **Internas**: `WorkerHandle`, `WorkerSpec`, `WorkerState`, `ResourceUsage`, `WorkerTTLConfig`
- **Cross-context**: `JobId` (referencia opcional al job asignado)
- **Sin dependencias externas**: Puro dominio

**💡 Motivación**:
Se creó para:
1. Encapsular toda la lógica de estado de un worker en un solo lugar
2. Implementar máquina de estados robusta para transiciones de worker
3. Garantizar que el worker no exceda sus límites de recursos
4. Manejar timeouts y limpieza automática

**🎨 Patrones Aplicados**:
- **Máquina de Estados**: WorkerState define estados válidos y transiciones
- **Patrón Agregado**: Garantiza consistencia del worker
- **Cumplimiento de Invariantes**: Los métodos públicos aseguran estado válido
- **Gestión de Recursos**: Seguimiento de uso de recursos

```rust
// Raíz de Agregado
pub struct Worker {
    handle: WorkerHandle,
    spec: WorkerSpec,
    state: WorkerState,
    current_job_id: Option<JobId>,
    resources: ResourceUsage,
    heartbeat: Option<DateTime<Utc>>,
    ttl_config: WorkerTTLConfig,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

---

**Objetos de Valor: WorkerHandle, WorkerSpec**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada Objeto de Valor tiene una única responsabilidad de representar un concepto específico.
- **LSP**: Los Objetos de Valor son sustituibles por igualdad.

**🏗️ Ámbito en DDD**: **Componentes - Contexto Workers**
- Son inmutables por definición
- No tienen identidad propia, se identifican por sus valores
- `WorkerHandle`: Identificador único y referencia al proveedor
- `WorkerSpec`: Especificación completa de configuración del worker

**🔗 Dependencias**:
- `WorkerHandle`: `WorkerId`, `ProviderType`, `String`
- `WorkerSpec`: `ProviderId`, `ProviderType`, `WorkerResources`, `WorkerTTLConfig`
- **Cross-context**: `JobId` (referencia al job que motivó el aprovisionamiento)

**💡 Motivación**:
Se crearon para:
1. Capturar la complejidad de configuración de workers en objetos tipados
2. Evitar Obsesión por Primitivos (no usar strings/HashMaps crudos)
3. Facilitar validaciones centralizadas de configuraciones
4. Garantizar inmutabilidad de configuraciones críticas

**🎨 Patrones Aplicados**:
- **Patrón Value Object**: Objetos sin identidad, definidos por atributos
- **Seguridad de Tipos**: Enums en lugar de strings para ProviderType
- **Patrón Builder**: (implícito) Construcción gradual de specs complejas

```rust
// Objetos de Valor
pub struct WorkerHandle {
    worker_id: WorkerId,
    provider_type: ProviderType,
    provider_execution_id: String,
    connection_id: Option<String>,
}

pub struct WorkerSpec {
    provider_id: ProviderId,
    provider_type: ProviderType,
    image: String,
    resources: WorkerResources,
    ttl_config: WorkerTTLConfig,
    labels: HashMap<String, String>,
}
```

---

**Enums: ProviderType y WorkerState**

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada enum tiene una única responsabilidad: definir tipos válidos.
- **OCP**: Nuevos proveedores o estados pueden agregarse sin modificar código existente (con pattern matching).

**🏗️ Ámbito en DDD**: **Tipos Centrales - Contexto Workers**
- `ProviderType`: Define los tipos de infraestructura soportados
- `WorkerState`: Define los estados válidos en la máquina de estados del worker

**🔗 Dependencias**:
- **Sin dependencias**: Enums primitivos, sin dependencias externas
- **Auto-referencial**: WorkerState puede referenciar a sí mismo en transiciones

**💡 Motivación**:
Se crearon para:
1. **Seguridad de Tipos**: Compilador asegura que solo se usen tipos válidos
2. **Pattern Matching Exhaustivo**: Rust obliga a manejar todos los casos
3. **Documentación Inline**: Los enums sirven como documentación de dominio
4. **Prevenir Errores**: No hay strings mágicos, solo tipos válidos

**🎨 Patrones Aplicados**:
- **Enum con Seguridad de Tipos**: Alternativa a strings mágicos
- **Máquina de Estados**: WorkerState define estados válidos
- **Patrón Estrategia**: ProviderType permite diferentes estrategias de aprovisionamiento

```rust
pub enum ProviderType {
    Docker,
    Kubernetes,
    Firecracker,
}

pub enum WorkerState {
    Creating,
    Connecting,
    Ready,
    Busy,
    Draining,
    Terminating,
    Terminated,
}
```

// Traits
#[async_trait]
pub trait WorkerProvider: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    fn provider_type(&self) -> ProviderType;
    fn capabilities(&self) -> &ProviderCapabilities;

    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError>;
    // ... más métodos
}

#[async_trait]
pub trait WorkerRegistry: Send + Sync {
    async fn register(&self, handle: WorkerHandle, spec: WorkerSpec, job_id: JobId) -> Result<Worker>;
    async fn unregister(&self, worker_id: &WorkerId) -> Result<()>;
    async fn get(&self, worker_id: &WorkerId) -> Result<Option<Worker>>;
    async fn find_available(&self) -> Result<Vec<Worker>>;
    // ... más métodos
}
```

**Eventos de Dominio**:

```rust
pub enum WorkerEvent {
    WorkerRegistered {
        worker_id: WorkerId,
        provider_id: ProviderId,
        spec: WorkerSpec,
    },
    WorkerStatusChanged {
        worker_id: WorkerId,
        old_state: WorkerState,
        new_state: WorkerState,
    },
    WorkerHeartbeat {
        worker_id: WorkerId,
        resources: ResourceUsage,
    },
    WorkerTerminated {
        worker_id: WorkerId,
        reason: TerminationReason,
    },
    // ... más eventos
}
```

#### 3. Contexto de Proveedores (`domain/src/providers/`)

**Responsabilidad**: Configuración y gestión de proveedores

**Componentes Clave**:

```rust
// Raíz de Agregado
pub struct ProviderConfig {
    id: ProviderId,
    name: String,
    provider_type: ProviderType,
    config: ProviderSpecificConfig,
    status: ProviderStatus,
    capabilities: ProviderCapabilities,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub enum ProviderSpecificConfig {
    Docker(DockerConfig),
    Kubernetes(KubernetesConfig),
    Firecracker(FirecrackerConfig),
}

pub struct ProviderCapabilities {
    max_cpu_cores: Option<u32>,
    max_memory_gb: Option<u64>,
    supports_gpu: bool,
    supported_runtimes: Vec<String>,
    supported_architectures: Vec<String>,
    max_concurrent_jobs: Option<u32>,
}

#[async_trait]
pub trait ProviderConfigRepository: Send + Sync {
    async fn save(&self, config: &ProviderConfig) -> Result<()>;
    async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>>;
    async fn find_enabled(&self) -> Result<Vec<ProviderConfig>>;
    // ... más métodos
}
```

#### 4. Contexto de Planificación (`domain/src/scheduling/`)

**Responsabilidad**: Estrategias de asignación trabajo-a-worker

**Componentes Clave**:

```rust
pub trait Scheduler: Send + Sync {
    async fn select_worker(
        &self,
        job: &Job,
        available_workers: &[Worker],
        providers: &[ProviderConfig],
    ) -> Result<Option<WorkerId>, SchedulingError>;
}

pub struct SmartScheduler {
    strategy: Box<dyn SchedulingStrategy>,
    policy: SchedulingPolicy,
}

pub trait SchedulingStrategy: Send + Sync {
    fn score(&self, job: &Job, worker: &Worker, provider: &ProviderConfig) -> f64;
}

pub enum SchedulingStrategy {
    RoundRobin,
    LeastLoaded,
    PriorityBased,
    ResourceAware,
    CostOptimized,
}
```

#### 5. Contexto de Plantillas (`domain/src/templates/`)

**Responsabilidad**: Gestión de plantillas de trabajo (EPIC-34)

**Componentes Clave**:

```rust
pub struct JobTemplate {
    id: TemplateId,
    name: String,
    description: Option<String>,
    spec: JobSpec,
    parameters: Vec<TemplateParameter>,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub struct TemplateParameter {
    name: String,
    param_type: ParameterType,
    default_value: Option<serde_json::Value>,
    required: bool,
    description: Option<String>,
}

#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn save(&self, template: &JobTemplate) -> Result<()>;
    async fn find_by_id(&self, id: &TemplateId) -> Result<Option<JobTemplate>>;
    async fn find_by_name(&self, name: &Str) -> Result<Option<JobTemplate>>;
    // ... más métodos
}
```

#### 6. Contexto IAM (`domain/src/iam/`)

**Responsabilidad**: Autenticación y autorización

**Componentes Clave**:

```rust
pub struct BootstrapToken {
    token: OtpToken,
    worker_id: WorkerId,
    provider_id: ProviderId,
    spec_hash: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[async_trait]
pub trait OtpTokenStore: Send + Sync {
    async fn generate(&self, worker_id: WorkerId, provider_id: ProviderId, ttl: Duration) -> Result<BootstrapToken>;
    async fn validate(&self, token: &OtpToken) -> Result<WorkerRegistration>;
    async fn consume(&self, token: &OtpToken) -> Result<()>;
}
```

#### 7. Contexto de Auditoría (`domain/src/audit/`)

**Responsabilidad**: Registro de auditoría

**Componentes Clave**:

```rust
pub struct AuditLog {
    id: AuditLogId,
    actor: String,
    action: String,
    target_type: String,
    target_id: String,
    changes: Option<serde_json::Value>,
    timestamp: DateTime<Utc>,
    metadata: AuditMetadata,
}

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn log(&self, entry: &AuditLog) -> Result<()>;
    async fn find_by_actor(&self, actor: &str, limit: usize) -> Result<Vec<AuditLog>>;
    // ... más métodos
}
```

#### 8. Contexto de Credenciales (`domain/src/credentials/`)

**Responsabilidad**: Gestión de secretos y credenciales

**Componentes Clave**:

```rust
pub struct Secret {
    id: SecretId,
    name: String,
    value: SecretValue,
    secret_type: SecretType,
    version: u32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[async_trait]
pub trait SecretProvider: Send + Sync {
    async fn get(&self, name: &str) -> Result<Secret>;
    async fn list(&self) -> Result<Vec<Secret>>;
    // ... más métodos
}
```

#### 9. Contexto Saga (`domain/src/saga/`)

**Responsabilidad**: Coordinación de transacciones distribuidas

**Componentes Clave**:

```rust
pub struct Saga<Steps> {
    id: SagaId,
    state: SagaState,
    current_step: usize,
    steps: Steps,
    compensation_data: HashMap<String, serde_json::Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub enum SagaState {
    Pending,
    InProgress,
    Compensating,
    Completed,
    Failed,
}

#[async_trait]
pub trait SagaStep: Send + Sync {
    type Input;
    type Output;
    type CompensatingInput;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, SagaError>;
    async fn compensate(&self, input: Self::CompensatingInput) -> Result<(), SagaError>;
}

#[async_trait]
pub trait SagaRepository: Send + Sync {
    async fn save(&self, saga: &Saga<dyn Any>) -> Result<()>;
    async fn find_by_id(&self, id: &SagaId) -> Result<Option<Saga<dyn Any>>>;
    async fn find_pending(&self) -> Result<Vec<Saga<dyn Any>>>;
    // ... más métodos
}
```

### Shared Kernel (`domain/src/shared_kernel/`)

**Propósito**: Tipos y conceptos compartidos a través de todos los contextos delimitados

```rust
// IDs
pub struct JobId(pub Uuid);
pub struct WorkerId(pub Uuid);
pub struct ProviderId(pub Uuid);
pub struct TemplateId(pub Uuid);
pub struct SagaId(pub Uuid);

// Estados
pub enum JobState {
    Pending,
    Scheduled,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Timeout,
}

pub enum WorkerState {
    Creating,
    Connecting,
    Ready,
    Busy,
    Draining,
    Terminating,
    Terminated,
}

// Errores
pub type Result<T> = std::result::Result<T, DomainError>;

pub enum DomainError {
    JobNotFound(JobId),
    WorkerNotFound(WorkerId),
    ProviderNotFound(ProviderId),
    InvalidStateTransition { current: String, target: String },
    ProviderUnhealthy(ProviderId),
    MaxAttemptsExceeded { job_id: JobId, attempts: u32 },
    // ... más errores
}
```

---

## Capa de Aplicación (Servidor)

### Propósito

La capa de aplicación orquesta **casos de uso** utilizando lógica de dominio. Coordina entre contextos delimitados e infraestructura.

### Infraestructura CQRS (`application/src/core/`)

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada trait tiene una única responsabilidad: `CommandBus` para comandos, `QueryBus` para queries, `Command`/`Query` para tipar mensajes.
- **ISP**: Los clientes dependen solo de las interfaces que usan (CommandBus o QueryBus, no ambos).
- **DIP**: Las capas superiores dependen de estas abstracciones, no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Capa de Aplicación - Patrón CQRS**
- Implementa el patrón **Segregación de Responsabilidad de Comando y Consulta**
- Separa operaciones de escritura (comandos) de lectura (queries)
- Permite optimizar cada lado independientemente (caching, sharding, etc.)

**🔗 Dependencias**:
- **Tipos de Dominio**: `Result<T>` del shared kernel
- **Concurrencia**: `Send + Sync` para seguridad de hilos
- **Genéricos**: `C: Command`, `Q: Query` para seguridad de tipos
- **Sin dependencias externas**: Puro Rust standard library

**💡 Motivación**:
Se creó para:
1. **Implementar CQRS**: Separar claramente comandos y queries
2. **Seguridad de Tipos**: Compilador asegura tipos correctos de comandos y resultados
3. **Extensibilidad**: Fácil agregar nuevos comandos y queries
4. **Testing**: Mocks simples para CommandBus y QueryBus
5. **Rendimiento**: Permitir optimizaciones específicas por tipo de operación

**🎨 Patrones Aplicados**:
- **Patrón CQRS**: Separación de responsabilidad de comandos y queries
- **Patrón Bus**: Desacoplamiento entre emisor y receptor de mensajes
- **Programación Genérica**: Traits genéricos para seguridad de tipos en tiempo de compilación
- **Inversión de Dependencia**: Depende de traits, no de implementaciones

**⚠️ Consideraciones de Diseño**:
- **Async/Await**: Todos los métodos son asíncronos para no bloquear
- **Manejo de Errores**: Usa `Result<T>` para errores tipados
- **Borrado de Tipos**: El trait object permite despacho dinámico

```rust
// Bus de Comandos
pub trait CommandBus: Send + Sync {
    async fn dispatch<C>(&self, command: C) -> Result<C::Result>
    where
        C: Command + Send + 'static;
}

// Bus de Consultas
pub trait QueryBus: Send + Sync {
    async fn execute<Q>(&self, query: Q) -> Result<Q::Result>
    where
        Q: Query + Send + 'static;
}

// Trait Comando
pub trait Command: Send + Sync {
    type Result: Send + Sync;
}

// Trait Consulta
pub trait Query: Send + Sync {
    type Result: Send + Sync;
}
```

### Casos de Uso de Jobs (`application/src/jobs/`)

**🏗️ Ámbito en DDD**: **Capa de Aplicación - Contexto Jobs**
- Orquesta casos de uso relacionados con jobs
- Coordina entre contextos delimitados (Jobs, Workers, Proveedores)
- Implementa flujos de negocio complejos que cruzan agregados

**📋 Responsabilidad General (SOLID)**:
- **SRP**: Cada Caso de Uso tiene una única responsabilidad (crear, despachar, controlar)
- **OCP**: Abierto para extender con nuevos Casos de Uso sin modificar existentes
- **DIP**: Depende de abstracciones de dominio, no de implementaciones concretas

#### 1. CreateJobUseCase

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: crear y validar jobs según reglas de negocio.
- **OCP**: Abierto para agregar nuevas validaciones sin modificar el caso de uso existente.
- **DIP**: Depende de `JobQueue`, `JobRepository`, `Scheduler`, `EventBus` (abstracciones), no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Capa de Aplicación - Contexto Jobs**
- Orquesta la creación de jobs interactuando con múltiples contextos delimitados
- Coordina entre: Jobs (crear), Cola (encolar), Planificación (seleccionar), Workers (aprovisionar)

**🔗 Dependencias**:
- **Dominio (Jobs)**: `JobQueue`, `JobRepository`
- **Dominio (Planificación)**: `Scheduler`
- **Dominio (Bus de Eventos)**: `EventBus`
- **Concurrencia**: `Arc<T>` para compartir estado entre hilos

**💡 Motivación**:
Se creó para:
1. **Encapsular flujo de negocio**: Creación de job es más que simple INSERT
2. **Orquestar múltiples contextos delimitados**: Validar, crear, encolar, programar
3. **Publicar eventos**: Notificar al sistema sobre nuevo job
4. **Manejar errores**: Traducir errores técnicos a errores de dominio

**🎨 Patrones Aplicados**:
- **Patrón Caso de Uso**: Encapsula un caso de uso específico del dominio
- **Patrón Orquestación**: Coordina múltiples contextos delimitados
- **Inyección de Dependencia**: Recibe dependencias por constructor
- **Publicación de Eventos**: Publica eventos al finalizar para integración

**⚠️ Consideraciones de Diseño**:
- **Límite Transaccional**: Debe ser transaccional (crear + encolar + publicar)
- **Manejo de Errores**: Debe traducir todos los errores a `Result<Job>`
- **Validación**: Valida en la aplicación, no en la capa de infraestructura

```rust
pub struct CreateJobUseCase {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    scheduler: Arc<dyn Scheduler>,
    event_bus: Arc<dyn EventBus>,
}

impl CreateJobUseCase {
    pub async fn execute(&self, request: CreateJobRequest) -> Result<Job> {
        // 1. Validar solicitud
        // 2. Crear agregado job
        // 3. Persistir job
        // 4. Publicar evento JobCreated
        // 5. Disparar aprovisionamiento si es necesario
    }
}
```

---

#### 2. JobController

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: bucle de control que procesa jobs pendientes continuamente.
- **OCP**: Abierto para agregar nuevas estrategias de procesamiento sin modificar el bucle principal.
- **DIP**: Depende de abstracciones, no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Capa de Aplicación - Contexto Jobs**
- Implementa el **bucle de control** que mantiene el sistema procesando jobs
- Coordinador principal entre: Cola, Workers, Despachador, Eventos

**🔗 Dependencias**:
- **Dominio (Jobs)**: `JobQueue`, `JobRepository`
- **Dominio (Workers)**: `WorkerRegistry`
- **Aplicación**: `JobDispatcher`
- **Dominio (Bus de Eventos)**: `EventBus`

**💡 Motivación**:
Se creó para:
1. **Implementar patrón reactor**: Reaccionar continuamente a cambios en cola
2. **Separar preocupaciones**: Bucle de control vs lógica de despacho
3. **Facilitar testing**: Bucle puede testearse aisladamente
4. **Monitoreo**: Punto central para métricas de procesamiento

**🎨 Patrones Aplicados**:
- **Patrón Controlador**: Controla el flujo principal del sistema
- **Patrón Reactor**: Reacciona a eventos (jobs en cola)
- **Bucle Async**: Usa `tokio::spawn` para concurrencia no bloqueante
- **Inyección de Dependencia**: Todas las dependencias inyectadas

**⚠️ Consideraciones de Diseño**:
- **Contrapresión**: Debe manejar cuando no hay workers disponibles
- **Recuperación de Errores**: Debe recuperar elegantemente de errores en el bucle
- **Apagado Elegante**: Debe poder detenerse limpiamente
- **Idempotencia**: Procesar el mismo job múltiples veces no debe causar problemas

```rust
pub struct JobController {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    dispatcher: Arc<JobDispatcher>,
    event_bus: Arc<dyn EventBus>,
}

impl JobController {
    pub async fn start(&self) -> Result<()> {
        // Iniciar bucle de control
        tokio::spawn(async move {
            loop {
                self.process_pending_jobs().await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    async fn process_pending_jobs(&self) -> Result<()> {
        let pending = self.job_queue.peek().await?;
        if let Some(job) = pending {
            self.dispatch_job(job).await?;
        }
        Ok(())
    }
}
```

#### 3. JobDispatcher

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: asignar jobs a workers según políticas de planificación.
- **OCP**: Abierto para agregar nuevas estrategias de despacho sin modificar lógica principal.
- **DIP**: Depende de abstracciones (`WorkerRegistry`, `JobRepository`, `EventBus`), no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Capa de Aplicación - Coordinación Cross-Context**
- Coordina entre contextos delimitados de Jobs y Workers
- Implementa la lógica de **asignación** de jobs a workers
- Publica eventos para integración con otros contextos

**🔗 Dependencias**:
- **Dominio (Workers)**: `WorkerRegistry`
- **Dominio (Jobs)**: `JobRepository`
- **Dominio (Proveedores)**: `ProviderRegistry`
- **Dominio (Bus de Eventos)**: `EventBus`
- **Planificación**: Algoritmos de selección de worker

**💡 Motivación**:
Se creó para:
1. **Separar preocupaciones**: Planificación es lógica compleja que merece su propio componente
2. **Encapsular políticas**: Todas las decisiones de asignación en un solo lugar
3. **Facilitar testing**: Lógica de despacho puede testearse aisladamente
4. **Publicar eventos**: Notificar al sistema cuando un job es asignado

**🎨 Patrones Aplicados**:
- **Patrón Despachador**: Asigna jobs a workers según políticas
- **Patrón Estrategia**: Diferentes estrategias de asignación (menos cargado, round robin, etc.)
- **Publicación de Eventos**: Notifica cambios de estado al sistema
- **Manejo de Errores**: Maneja casos donde no hay workers disponibles

**⚠️ Consideraciones de Diseño**:
- **Condiciones de Carrera**: Debe manejar workers que cambian de estado durante despacho
- **Contrapresión**: Debe manejar cuando no hay workers disponibles
- **Timeouts**: Debe implementar timeouts para despacho
- **Idempotencia**: Despacho del mismo job múltiples veces debe ser idempotente

```rust
pub struct JobDispatcher {
    worker_registry: Arc<dyn WorkerRegistry>,
    job_repository: Arc<dyn JobRepository>,
    event_bus: Arc<dyn EventBus>,
    provider_registry: Arc<ProviderRegistry>,
}

impl JobDispatcher {
    pub async fn dispatch(&self, job: Job) -> Result<DispatchResult> {
        // 1. Encontrar worker disponible
        let worker = self.find_worker(&job).await?;

        // 2. Asignar job a worker
        self.assign_job(&worker, &job).await?;

        // 3. Enviar job a worker vía gRPC
        self.send_to_worker(&worker, &job).await?;

        // 4. Publicar evento JobDispatched
        self.event_bus.publish(JobDispatchedEvent {
            job_id: job.id.clone(),
            worker_id: worker.handle().worker_id.clone(),
        }).await?;

        Ok(DispatchResult::Success)
    }
}
```

### Casos de Uso de Workers (`application/src/workers/`)

#### 1. WorkerProvisioningService

```rust
pub struct WorkerProvisioningService {
    worker_registry: Arc<dyn WorkerRegistry>,
    provider_registry: Arc<ProviderRegistry>,
    otp_store: Arc<dyn OtpTokenStore>,
    event_bus: Arc<dyn EventBus>,
}

impl WorkerProvisioningService {
    pub async fn provision_worker(&self, job: &Job) -> Result<Worker> {
        // 1. Seleccionar proveedor basado en requerimientos del job
        let provider = self.select_provider(job).await?;

        // 2. Crear especificación de worker
        let spec = self.create_worker_spec(job, &provider).await?;

        // 3. Generar token OTP
        let token = self.otp_store.generate(
            WorkerId::new(),
            provider.id.clone(),
            Duration::from_secs(300),
        ).await?;

        // 4. Aprovisionar worker vía proveedor
        let handle = provider.create_worker(&spec).await?;

        // 5. Registrar worker
        let worker = self.worker_registry.register(
            handle.clone(),
            spec,
            job.id.clone(),
        ).await?;

        // 6. Publicar evento WorkerProvisioned
        self.event_bus.publish(WorkerProvisionedEvent {
            worker_id: worker.handle().worker_id.clone(),
            provider_id: provider.id.clone(),
        }).await?;

        Ok(worker)
    }
}
```

#### 2. WorkerSupervisor (Modelo de Actores)

```rust
pub struct WorkerSupervisor {
    workers: Arc<DashMap<WorkerId, WorkerActor>>,
    event_bus: Arc<dyn EventBus>,
    config: WorkerSupervisorConfig,
}

pub struct WorkerActor {
    worker_id: WorkerId,
    state: WorkerState,
    heartbeat_timeout: Duration,
    last_heartbeat: Instant,
}

impl WorkerSupervisor {
    pub async fn handle_heartbeat(&self, worker_id: WorkerId, heartbeat: WorkerHeartbeat) {
        if let Some(actor) = self.workers.get(&worker_id) {
            actor.update_heartbeat(heartbeat).await;
        }
    }

    pub async fn monitor_timeouts(&self) {
        tokio::spawn(async move {
            loop {
                for entry in self.workers.iter() {
                    let actor = entry.value();
                    if actor.is_timeout().await {
                        self.handle_timeout(actor.worker_id.clone()).await;
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }
}
```

#### 3. AutoScalingService

```rust
pub struct AutoScalingService {
    worker_registry: Arc<dyn WorkerRegistry>,
    job_queue: Arc<dyn JobQueue>,
    provider_registry: Arc<ProviderRegistry>,
    config: AutoScalingConfig,
}

impl AutoScalingService {
    pub async fn evaluate_scaling(&self) -> Result<ScalingDecision> {
        // 1. Obtener longitud de cola
        let queue_len = self.job_queue.len().await?;

        // 2. Obtener workers disponibles
        let available_workers = self.worker_registry.find_available().await?;

        // 3. Calcular conteo deseado de workers
        let desired = self.calculate_desired_count(queue_len, available_workers.len());

        // 4. Escalar hacia arriba o abajo
        if desired > available_workers.len() as i32 {
            self.scale_up((desired - available_workers.len() as i32) as usize).await?;
        } else if desired < available_workers.len() as i32 {
            self.scale_down((available_workers.len() as i32 - desired) as usize).await?;
        }

        Ok(ScalingDecision {
            current: available_workers.len(),
            desired,
        })
    }
}
```

### Casos de Uso Saga (`application/src/saga/`)

#### 1. ProvisioningSagaCoordinator

```rust
pub struct ProvisioningSagaCoordinator {
    orchestrator: Arc<SagaOrchestrator>,
    worker_provisioning: Arc<WorkerProvisioningService>,
    worker_registry: Arc<dyn WorkerRegistry>,
}

impl ProvisioningSagaCoordinator {
    pub async fn start_provisioning(&self, job_id: JobId) -> Result<SagaId> {
        let saga = Saga::builder()
            .step(GenerateOTPStep::new())
            .step(ProvisionWorkerStep::new())
            .step(WaitForRegistrationStep::new())
            .build();

        self.orchestrator.execute(saga).await
    }
}
```

#### 2. ExecutionSagaDispatcher

```rust
pub struct ExecutionSagaDispatcher {
    orchestrator: Arc<SagaOrchestrator>,
    job_dispatcher: Arc<JobDispatcher>,
    job_repository: Arc<dyn JobRepository>,
}
```

---

## Capa de Infraestructura (Servidor)

### Propósito

La capa de infraestructura proporciona implementaciones concretas para las interfaces definidas en la capa de dominio. Maneja toda la E/S externa (base de datos, red, sistema de archivos).

### Persistencia (`infrastructure/src/persistence/`)

**Repositorios Implementados**:
- `PostgresJobRepository`: Usa `sqlx` para almacenar jobs en PostgreSQL
- `PostgresWorkerRegistry`: Almacena estado del worker
- `PostgresProviderConfigRepository`: Almacena configuraciones de proveedores
- `PostgresAuditLogRepository`: Almacena logs de auditoría

**Características Clave**:
- **Pooling de Conexiones**: Usa `sqlx::PgPool`
- **Migraciones**: Gestionadas vía `sqlx-cli`
- **Transacciones**: Soporta transacciones ACID
- **JSONB**: Usa PostgreSQL JSONB para esquema flexible (specs, recursos)

### Proveedores (`infrastructure/src/providers/`)

#### 1. Proveedor Docker (`infrastructure/src/providers/docker.rs`)
- **Librería**: `bollard`
- **Características**:
  - Creación/eliminación de contenedores
  - Gestión de red
  - Montaje de volúmenes
  - Límites de recursos (CPU/Memoria)

#### 2. Proveedor Kubernetes (`infrastructure/src/providers/kubernetes/`)
- **Librería**: `kube-rs`
- **Características**:
  - Gestión del ciclo de vida de Pods
  - Gestión de secretos
  - Integración con ConfigMap
  - Integración con Service Account

#### 3. Proveedor Firecracker (`infrastructure/src/providers/firecracker/`)
- **Librería**: Wrapper personalizado alrededor de API Firecracker
- **Características**:
  - Ciclo de vida de microVM
  - Integración con Jailer
  - Gestión de dispositivo tap de red
  - Configuración de unidades

### Mensajería (`infrastructure/src/messaging/`)

#### 1. Outbox PostgreSQL
- **Mecanismo**: Escribe eventos en tabla `outbox` en la misma transacción que cambios de agregado
- **Poller**: Tarea en segundo plano consulta tabla `outbox` y publica al bus de eventos
- **Garantía**: Entrega al-menos-una-vez

#### 2. Bus de Eventos NATS (Opcional)
- **Librería**: `async-nats`
- **Uso**: Para comunicación entre servicios si se escala más allá de un solo nodo

### Observabilidad (`infrastructure/src/observability/`)

- **Trazas**: `tracing` + `opentelemetry`
- **Métricas**: `prometheus`
- **Logging**: `tracing-subscriber` (formato JSON)

---

## Capa de Interfaz (Servidor)

### Propósito

La capa de interfaz maneja la comunicación externa. Traduce solicitudes externas (gRPC, REST) en comandos/consultas de aplicación.

### Servicios gRPC (`interface/src/grpc/`)

- **Framework**: `tonic`
- **Servicios**:
  - `JobExecutionService`: `QueueJob`, `GetJobStatus`
  - `WorkerAgentService`: `Register`, `WorkerStream`
  - `SchedulerService`: `GetQueueStatus`
  - `LogStreamService`: `SubscribeLogs`

### Mappers (`interface/src/mappers/`)

- **Responsabilidad**: Convertir entre mensajes Proto y entidades de Dominio
- **Patrón**: Traits `From`/`TryFrom`

---

## Arquitectura del Worker

### Visión General

El worker es un agente ligero diseñado para ejecutar un único trabajo y terminar. Está construido para alto rendimiento y seguridad.

### Componentes

#### 1. Ejecutor de Trabajos (`worker/infrastructure/src/executor.rs`)
- **Responsabilidad**: Ejecutar el comando real
- **Características**:
  - Creación de procesos
  - Tuberías Stdin/Stdout/Stderr
  - Inyección de secretos vía stdin
  - Monitoreo de recursos

#### 2. Batcher de Logs (`worker/infrastructure/src/log_batcher.rs`)
- **Responsabilidad**: Bufferizar logs para reducir sobrecarga gRPC
- **Optimización**: Reduce llamadas gRPC en 90-99%
- **Config**: Tamaño de lote, intervalo de flush

#### 3. Inyector de Secretos (`worker/infrastructure/src/secret_injector.rs`)
- **Responsabilidad**: Inyectar secretos de forma segura
- **Mecanismo**: Escribe en stdin y lo cierra inmediatamente
- **Seguridad**: Los secretos nunca tocan disco o variables de entorno

#### 4. Recolector de Métricas (`worker/infrastructure/src/metrics.rs`)
- **Responsabilidad**: Recolectar uso de CPU/Memoria
- **Optimización**: Caché de métricas por 35s para reducir sobrecarga

---

## Flujo de Ejecución de Trabajos

```mermaid
sequenceDiagram
    participant C as Cliente
    participant G as Servidor gRPC
    participant A as Aplicación
    participant D as Dominio
    participant I as Infraestructura
    participant P as Proveedor
    participant W as Worker

    Note over C,W: 1. Envío de Trabajo
    C->>G: QueueJob(JobSpec)
    G->>A: CreateJobUseCase.execute()
    A->>D: Job::new(spec)
    D-->>A: Agregado Job
    A->>I: JobRepository.save()
    A->>I: JobQueue.enqueue()
    A-->>G: Job creado
    G-->>C: QueueJobResponse(job_id)

    Note over C,W: 2. Aprovisionamiento de Worker
    A->>D: Verificar workers disponibles
    D-->>A: No hay workers disponibles
    A->>A: WorkerProvisioningService.provision()
    A->>D: OtpTokenStore.generate()
    D-->>A: BootstrapToken
    A->>P: create_worker(spec, otp)
    P->>P: Iniciar contenedor/pod
    P-->>A: WorkerHandle

    Note over C,W: 3. Registro de Worker (OTP)
    W->>G: Register(otp, worker_info)
    G->>A: ValidateOTP
    A->>D: OtpTokenStore.validate()
    D-->>A: WorkerRegistration
    A->>I: WorkerRegistry.register()
    A->>D: Publicar WorkerRegisteredEvent
    A-->>W: RegisterResponse(session_id)

    Note over C,W: 4. Asignación de Trabajo
    A->>D: Encontrar worker disponible
    D-->>A: Worker
    A->>D: AssignJobToWorker
    D-->>A: Estado Job → Running
    A->>I: JobRepository.save()

    Note over C,W: 5. Stream Bidireccional
    W->>G: WorkerStream(bidireccional)
    loop
        W->>G: WorkerHeartbeat(metrics)
        G-->>W: ACK
        G->>W: RunJobCommand(job_spec)
        W->>W: JobExecutor.execute()
        W->>G: LogBatch(logs en lote)
        W->>G: JobResult(resultado)
    end

    Note over C,W: 6. Completitud de Trabajo
    W->>G: JobResult(success=true)
    G->>A: JobCompletionHandler.handle()
    A->>D: Job.complete()
    D-->>A: Estado Job → Succeeded
    A->>I: JobRepository.save()
    A->>D: Publicar JobCompletedEvent
    A-->>W: Ack

    Note over C,W: 7. Terminación de Worker
    A->>D: Verificar si worker puede terminar
    D-->>A: Terminate(idle)
    A->>P: destroy_worker(handle)
    P->>P: Detener contenedor/pod
    A->>D: Publicar WorkerTerminatedEvent
```

### Transiciones de Estado

**Máquina de Estados de Job**:
```
Pending → Scheduled → Queued → Running → Succeeded
                                 ↓
                            Failed/Cancelled/Timeout
```

**Máquina de Estados de Worker**:
```
Creating → Connecting → Ready → Busy → Ready → Draining → Terminating → Terminated
```

---

## Gestión del Ciclo de Vida del Worker

### Modelo de Actores (EPIC-42)

```mermaid
graph TB
    subgraph "WorkerSupervisor"
        SUPER[Supervisor]
        ACTORS[WorkerActors]
    end

    subgraph "WorkerActor"
        STATE[Estado]
        HEARTBEAT[Rastreador Heartbeat]
        TIMEOUT[Monitor Timeout]
    end

    SUPER -->|crea| ACTORS
    ACTORS --> STATE
    ACTORS --> HEARTBEAT
    ACTORS --> TIMEOUT

    HEARTBEAT -->|actualiza| STATE
    TIMEOUT -->|detecta| STATE
    STATE -->|dispara| TIMEOUT
```

### Implementación de WorkerActor

```rust
pub struct WorkerActor {
    worker_id: WorkerId,
    state: WorkerState,
    heartbeat_timeout: Duration,
    last_heartbeat: Arc<Mutex<Instant>>,
}

impl WorkerActor {
    pub async fn handle_heartbeat(&self, heartbeat: WorkerHeartbeat) {
        *self.last_heartbeat.lock().await = Instant::now();
    }

    pub async fn is_timeout(&self) -> bool {
        let last = *self.last_heartbeat.lock().await;
        last.elapsed() > self.heartbeat_timeout
    }

    pub async fn terminate(&self) {
        // Disparar lógica de terminación
    }
}
```

### Lógica de Auto-escalado

```rust
impl AutoScalingService {
    async fn evaluate_scaling(&self) -> Result<()> {
        let queue_len = self.job_queue.len().await?;
        let available = self.worker_registry.find_available().await?;

        let desired = self.calculate_desired(queue_len, available.len());

        if desired > available.len() {
            self.scale_up(desired - available.len()).await?;
        } else if desired < available.len() {
            self.scale_down(available.len() - desired).await?;
        }

        Ok(())
    }

    fn calculate_desired(&self, queue_len: usize, available: usize) -> usize {
        let min = self.config.min_workers;
        let max = self.config.max_workers;

        let target = (queue_len as f32 * self.config.scale_factor).ceil() as usize;
        target.clamp(min, max)
    }
}
```

---

## Proveedores de Workers

### Comparación

| Proveedor | Aislamiento | Inicio | GPU | Requerimientos | Caso de Uso |
|----------|----------|---------|-----|--------------|----------|
| **Docker** | Contenedor | ~1s | Sí | Daemon Docker | Desarrollo, CI/CD |
| **Kubernetes** | Contenedor (Pod) | ~5-15s | Sí | Cluster K8s | Producción, nativo de nube |
| **Firecracker** | Hardware (KVM) | ~125ms | No | Linux + KVM | Multi-tenant, crítico para seguridad |

### Configuración de Proveedor

**Docker**:
```bash
HODEI_DOCKER_ENABLED=1
HODEI_WORKER_IMAGE=hodei-worker:latest
HODEI_DOCKER_NETWORK=bridge
HODEI_DOCKER_CPUS=2.0
HODEI_DOCKER_MEMORY=4096
```

**Kubernetes**:
```bash
HODEI_K8S_ENABLED=1
HODEI_K8S_NAMESPACE=hodei-workers
HODEI_K8S_KUBECONFIG=/path/to/kubeconfig
HODEI_K8S_IMAGE_PULL_SECRET=registry-secret
HODEI_K8S_CPU_REQUEST=2
HODEI_K8S_MEMORY_REQUEST=4Gi
```

**Firecracker**:
```bash
HODEI_FC_ENABLED=1
HODEI_FC_KERNEL_PATH=/var/lib/hodei/vmlinux
HODEI_FC_ROOTFS_PATH=/var/lib/hodei/rootfs.ext4
HODEI_FC_USE_JAILER=true
HODEI_FC_CPU_COUNT=2
HODEI_FC_MEMORY_SIZE=4096
```

---

## Servicios gRPC

### Lista de Servicios

| Servicio | Descripción | RPCs |
|---------|-------------|------|
| **JobExecutionService** | Ciclo de vida del trabajo | QueueJob, GetJobStatus, CancelJob, ListJobs |
| **WorkerAgentService** | Registro y comunicación de worker | Register, WorkerStream, UnregisterWorker |
| **SchedulerService** | Decisiones de planificación | ScheduleJob, GetAvailableWorkers, ConfigureScheduler |
| **ProviderManagementService** | Gestión de proveedores | RegisterProvider, ListProviders, GetProviderHealth |
| **MetricsService** | Recolección de métricas | StreamMetrics, GetAggregatedMetrics |
| **LogStreamService** | Streaming de logs | SubscribeLogs, GetLogs |
| **TemplateService** | Plantillas de trabajo | CreateTemplate, GetTemplate, ListTemplates |

### Ejemplos de Mensajes

**QueueJob**:
```protobuf
message QueueJobRequest {
  JobDefinition job_definition = 1;
  string queued_by = 2;
}

message JobDefinition {
  oneof job_id {
    string value = 1;  // Auto-generar si está vacío
  }
  string name = 2;
  CommandType command = 3;
  JobResources resources = 4;
  JobPreferences preferences = 5;
}
```

**WorkerStream** (Bidireccional):
```protobuf
message WorkerMessage {
  oneof message {
    WorkerHeartbeat heartbeat = 1;
    LogEntry log_entry = 2;
    LogBatch log_batch = 3;  // Optimización v8.0
    JobResultMessage job_result = 4;
    WorkerStatsMessage stats = 5;
  }
}

message ServerMessage {
  oneof message {
    RunJobCommand run_job = 1;
    CancelJobCommand cancel_job = 2;
    AckMessage ack = 3;
    KeepAliveMessage keep_alive = 4;
  }
}
```

---

## Mensajería y Bus de Eventos

### Arquitectura

```mermaid
graph LR
    subgraph "Capa de Dominio"
        D[Eventos de Dominio]
    end

    subgraph "Capa de Aplicación"
        UC[Casos de Uso]
    end

    subgraph "Capa de Infraestructura"
        OB[Patrón Outbox]
        PG[(PostgreSQL)]
        POLLER[Poller Outbox]
        NATS[Broker NATS]
    end

    subgraph "Manejadores de Eventos"
        H1[Manejador 1]
        H2[Manejador 2]
        H3[Manejador 3]
    end

    D --> UC
    UC --> OB
    OB --> PG
    POLLER --> PG
    POLLER --> NATS
    NATS --> H1
    NATS --> H2
    NATS --> H3
```

### Tipos de Eventos

**Eventos de Job**:
- `JobCreated`
- `JobStatusChanged`
- `JobCompleted`
- `JobFailed`
- `JobCancelled`
- `JobTimeout`

**Eventos de Worker**:
- `WorkerRegistered`
- `WorkerStatusChanged`
- `WorkerHeartbeat`
- `WorkerTerminated`
- `WorkerUnhealthy`

**Eventos de Proveedor**:
- `ProviderRegistered`
- `ProviderHealthChanged`
- `ProviderDisabled`

### Consumidores de Eventos

```rust
// Detector de Worker Huérfano
pub struct OrphanWorkerDetectorConsumer {
    worker_registry: Arc<dyn WorkerRegistry>,
    worker_provisioning: Arc<WorkerProvisioningService>,
}

#[async_trait]
impl EventHandler<WorkerTerminatedEvent> for OrphanWorkerDetectorConsumer {
    async fn handle(&self, event: &WorkerTerminatedEvent) -> Result<()> {
        // Verificar si el job estaba corriendo
        let worker = self.worker_registry.get(&event.worker_id).await?;
        if let Some(job_id) = worker.current_job_id() {
            // Re-encolar job
            self.job_queue.enqueue(job).await?;
        }
        Ok(())
    }
}

// Consumidor Saga de Ejecución
pub struct ExecutionSagaConsumer {
    saga_orchestrator: Arc<SagaOrchestrator>,
}

#[async_trait]
impl EventHandler<JobCreatedEvent> for ExecutionSagaConsumer {
    async fn handle(&self, event: &JobCreatedEvent) -> Result<()> {
        // Iniciar saga de ejecución
        self.saga_orchestrator.start_provisioning_saga(event.job_id).await
    }
}
```

---

## Persistencia

### Esquema de Base de Datos

**Tablas**:
- `jobs` - Agregados de Job
- `job_queue` - Cola de jobs (con SKIP LOCKED para concurrencia)
- `workers` - Agregados de Worker
- `providers` - Configuraciones de proveedores
- `bootstrap_tokens` - Tokens OTP
- `outbox` - Outbox de eventos
- `audit_logs` - Rastro de auditoría
- `sagas` - Estado de Saga
- `job_templates` - Plantillas de trabajo

### Migraciones

Ubicadas en: `crates/server/infrastructure/migrations/`

```sql
-- Tabla Jobs
CREATE TABLE jobs (
    id UUID PRIMARY KEY,
    spec JSONB NOT NULL,
    state INTEGER NOT NULL,
    selected_provider UUID REFERENCES providers(id),
    execution_context JSONB,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Cola de Jobs
CREATE TABLE job_queue (
    job_id UUID PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_job_queue_priority ON job_queue (priority DESC, created_at ASC);

-- Tabla Workers
CREATE TABLE workers (
    worker_id UUID PRIMARY KEY,
    handle JSONB NOT NULL,
    spec JSONB NOT NULL,
    state INTEGER NOT NULL,
    current_job_id UUID REFERENCES jobs(id),
    resources JSONB,
    heartbeat TIMESTAMP,
    ttl_config JSONB,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Patrón Outbox
CREATE TABLE outbox (
    event_id UUID PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    published_at TIMESTAMP
);

CREATE INDEX idx_outbox_published ON outbox (published, created_at);
```

---

## Seguridad (Zero Trust)

### Infraestructura mTLS

```mermaid
graph TB
    CA[Certificado Raíz CA]
    SERVER[Certificado Servidor]
    CLIENT[Certificado Cliente]

    CA --> SERVER
    CA --> CLIENT

    SERVER -->|Handshake TLS| CLIENT
    CLIENT -->|Verificar Certificado| SERVER
```

### Gestión de Certificados

**Rutas**:
- CA: `/etc/hodei/pki/ca.crt`
- Cert servidor: `/etc/hodei/pki/server.crt`
- Key servidor: `/etc/hodei/pki/server.key`
- Cert cliente: `/etc/hodei/pki/client.crt`
- Key cliente: `/etc/hodei/pki/client.key`

**Script de Generación**: `scripts/generate-certificates.sh`

### Autenticación OTP

```rust
pub struct BootstrapToken {
    token: OtpToken,
    worker_id: WorkerId,
    provider_id: ProviderId,
    expires_at: DateTime<Utc>,
}

impl OtpTokenStore {
    pub async fn generate(&self, worker_id: WorkerId, provider_id: ProviderId, ttl: Duration) -> Result<BootstrapToken> {
        let token = OtpToken::generate();
        let expires_at = Utc::now() + ttl;

        let bootstrap = BootstrapToken {
            token: token.clone(),
            worker_id,
            provider_id,
            expires_at,
        };

        self.store.save(&bootstrap).await?;
        Ok(bootstrap)
    }

    pub async fn validate(&self, token: &OtpToken) -> Result<WorkerRegistration> {
        let bootstrap = self.store.find_by_token(token).await?
            .ok_or(DomainError::InvalidToken)?;

        if bootstrap.expires_at < Utc::now() {
            return Err(DomainError::TokenExpired);
        }

        Ok(WorkerRegistration {
            worker_id: bootstrap.worker_id,
            provider_id: bootstrap.provider_id,
        })
    }
}
```

### Inyección de Secretos

```rust
impl JobExecutor {
    async fn inject_secrets(&self, secrets: &HashMap<String, String>) -> Result<()> {
        let json = serde_json::to_string(secrets)?;

        // Escribir a stdin (cerrado después de inyección)
        let mut stdin = self.child.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;

        stdin.write_all(json.as_bytes()).await?;
        stdin.shutdown().await?;

        Ok(())
    }
}
```

**Características de Seguridad**:
- Los secretos nunca aparecen en logs (redacción automática)
- Secretos transmitidos vía stdin con cierre inmediato
- Serialización JSON para múltiples secretos
- Registro de auditoría de acceso a secretos

---

## Optimizaciones de Rendimiento (v8.0)

### 1. LogBatching (Reducción 90-99% gRPC)

```rust
pub struct LogBatcher {
    buffer: Vec<LogEntry>,
    capacity: usize,  // Default: 100
    flush_interval: Duration,  // Default: 100ms
    last_flush: Instant,
}

// Beneficios:
// - Reducción 90-99% en llamadas gRPC
// - Transmisión por lotes (1 lote vs 100+ llamadas individuales)
// - Flush automático por capacidad o timeout
```

### 2. E/S Zero-Copy

```rust
// FramedRead + BytesCodec para zero-copy
let mut framed = FramedRead::new(source, BytesCodec::new());
while let Some(chunk) = framed.next().await {
    let bytes = chunk?;  // Slice Bytes directo - sin copia
    // Procesar directamente desde Bytes
}

// Beneficios:
// - Zero-copy de datos de log
// - BytesCodec para decodificación eficiente
// - ~40% reducción de asignación de memoria
```

### 3. Manejo de Contrapresión (Backpressure)

```rust
// Operaciones asíncronas no bloqueantes con try_send()
let _ = self.tx.try_send(message);

// Beneficios:
// - Previene bloqueo del runtime asíncrono
// - Descarta mensajes cuando el canal está lleno
// - 100% estabilidad del runtime asíncrono
```

### 4. Métricas en Caché (35s TTL)

```rust
pub struct CachedResourceUsage {
    usage: ResourceUsage,
    timestamp: Instant,
}

const METRICS_CACHE_TTL_SECS: u64 = 35;

// Beneficios:
// - ~60% reducción de sobrecarga de recolección de métricas
// - spawn_blocking para tareas intensivas
// - Recolección de métricas no bloqueante
```

### 5. Patrón Write-Execute

```rust
// Ejecución robusta de scripts con encabezados de seguridad
const SAFETY_HEADERS = r#"
set -e        # Salir en error
set -u        # Error en variables no definidas
set -o pipefail  # Salir en fallo de tubería
"#;

// Beneficios:
// - Inyección automática de encabezados de seguridad
// - Gestión segura de archivos temporales
// - Limpieza asíncrona (no bloqueante)
// - Manejo robusto de errores
```

### Métricas de Rendimiento

| Optimización | Mejora | Métrica |
|--------------|-------------|--------|
| **LogBatching** | 90-99% | reducción llamadas gRPC |
| **E/S Zero-Copy** | ~40% | reducción asignación memoria |
| **Métricas en Caché** | ~60% | sobrecarga recolección métricas |
| **Manejo Contrapresión** | 100% | estabilidad runtime asíncrono |
| **Patrón Write-Execute** | N/A | robustez ejecución scripts |

---

## Estrategia de Pruebas

### Niveles de Prueba

1. **Pruebas Unitarias**: Lógica de dominio, funciones puras
2. **Pruebas de Integración**: Casos de uso, repositorios
3. **Pruebas E2E**: Ejecución completa de trabajo con proveedores reales
4. **Pruebas de Contrato**: Contratos de servicio gRPC

### Organización de Pruebas

```
crates/
├── server/
│   ├── domain/src/**/*.rs          # Pruebas unitarias en mod.rs
│   ├── application/tests/          # Pruebas de integración
│   ├── infrastructure/tests/        # Pruebas de infraestructura
│   └── tests/                      # Pruebas E2E
│       ├── common/                 # Fixtures de prueba
│       ├── docker_integration.rs    # Pruebas proveedor Docker
│       ├── k8s_integration.rs      # Pruebas proveedor K8s
│       └── firecracker_integration.rs  # Pruebas Firecracker
└── worker/
    ├── bin/tests/                  # Pruebas E2E Worker
    └── infrastructure/tests/        # Pruebas infraestructura Worker
```

### Infraestructura de Pruebas

**Utilidades de Prueba** (`infrastructure/src/test_infrastructure.rs`):
- Repositorios en memoria
- Mock bus de eventos
- Fábricas de prueba

**Fixtures** (`server/tests/common/`):
- Fixtures de Job
- Fixtures de Worker
- Fixtures de Proveedor

### Ejecución de Pruebas

```bash
# Pruebas unitarias
cargo test --workspace

# Pruebas de integración
cargo test --test docker_integration -- --ignored

# Pruebas E2E
cargo test --test e2e_docker_provider -- --ignored --nocapture

# Todas las pruebas con cobertura
cargo test --workspace && cargo tarpaulin --out Html
```

---

## Gestión de Configuración

### Variables de Entorno

**Base de Datos**:
```bash
HODEI_DATABASE_URL=postgres://user:pass@localhost:5432/hodei
HODEI_DB_MAX_CONNECTIONS=10
HODEI_DB_CONNECTION_TIMEOUT_SECS=30
```

**Servidor gRPC**:
```bash
HODEI_SERVER_HOST=0.0.0.0
HODEI_GRPC_PORT=50051
HODEI_DEV_MODE=1
```

**Proveedores**:
```bash
HODEI_DOCKER_ENABLED=1
HODEI_K8S_ENABLED=1
HODEI_FC_ENABLED=0
```

**Worker**:
```bash
HODEI_SERVER=http://localhost:50051
HODEI_WORKER_HEARTBEAT_INTERVAL=30s
HODEI_WORKER_OTP_TOKEN_FILE=/var/run/hodei/otp.txt
```

### Archivos de Configuración

**Servidor**: `config/server.toml`
```toml
[server]
host = "0.0.0.0"
grpc_port = 50051
rest_port = 8080

[database]
url = "postgres://localhost:5432/hodei"
max_connections = 10

[providers.docker]
enabled = true
image = "hodei-worker:latest"

[auto_scaling]
min_workers = 2
max_workers = 10
scale_factor = 0.5
```

**Worker**: `config/worker.toml`
```toml
[worker]
server_url = "http://localhost:50051"
heartbeat_interval = "30s"

[executor]
script_dir = "/tmp/hodei"
cleanup_timeout = "10s"

[metrics]
cache_ttl_secs = 35
```

---

## Arquitectura de Despliegue

### Despliegue Docker

```yaml
# docker-compose.dev.yml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: hodei
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
    ports:
      - "5432:5432"

  server:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "50051:50051"
      - "8080:8080"
    environment:
      HODEI_DATABASE_URL: postgres://postgres:postgres@postgres:5432/hodei
      HODEI_DOCKER_ENABLED: 1
      HODEI_DEV_MODE: 1
    depends_on:
      - postgres
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock

  worker:
    build:
      context: .
      dockerfile: Dockerfile.worker
    environment:
      HODEI_SERVER: http://server:50051
    depends_on:
      - server
```

### Despliegue Kubernetes

```yaml
# deploy/kubernetes/server.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hodei-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: hodei-server
  template:
    metadata:
      labels:
        app: hodei-server
    spec:
      containers:
      - name: server
        image: hodei-server:latest
        ports:
        - containerPort: 50051
        env:
        - name: HODEI_DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: hodei-secrets
              key: database-url
        - name: HODEI_K8S_ENABLED
          value: "1"
```

### Helm Chart

Ubicado en: `helm/hodei-jobs/`

```bash
# Instalar
helm install hodei-jobs ./helm/hodei-jobs

# Actualizar
helm upgrade hodei-jobs ./helm/hodei-jobs

# Desinstalar
helm uninstall hodei-jobs
```

---

## Casos de Uso Detallados con Ejemplos

### Caso de Uso 1: Enviar y Ejecutar un Trabajo Simple

**Escenario**: Enviar un trabajo de comando shell y monitorear su ejecución

```bash
# 1. Enviar trabajo vía gRPC
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "my-job-001"},
    "name": "Hola Mundo",
    "command": {
      "shell": {
        "cmd": "echo",
        "args": ["Hola desde Hodei!"]
      }
    },
    "resources": {
      "cpu_cores": 1.0,
      "memory_mb": 512,
      "storage_mb": 1024
    },
    "preferences": {
      "allow_retry": true,
      "priority": 0
    }
  },
  "queued_by": "equipo-dev"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Respuesta
{
  "job_id": {"value": "550e8400-e29b-41d4-a716-446655440000"},
  "status": 2,  // Queued
  "created_at": "2026-01-09T10:00:00Z"
}

# 2. Monitorear estado del trabajo
grpcurl -plaintext -d '{
  "job_id": {"value": "550e8400-e29b-41d4-a716-446655440000"}
}' localhost:50051 hodei.JobExecutionService/GetJobStatus

# 3. Stream de logs
grpcurl -plaintext -d '{
  "job_id": {"value": "550e8400-e29b-41d4-a716-446655440000"}
}' localhost:50051 hodei.LogStreamService/SubscribeLogs
```

**Flujo Interno**:

```rust
// 1. Cliente envía QueueJobRequest
let request = QueueJobRequest { job_definition, queued_by };

// 2. Servidor gRPC llama caso de uso
let job = create_job_use_case.execute(request).await?;

// 3. Dominio crea agregado Job
let job = Job::new(job_spec, JobPreferences::default())?;

// 4. Persistir a base de datos
job_repository.save(&job).await?;
job_queue.enqueue(job.clone()).await?;

// 5. Publicar evento de dominio
event_bus.publish(JobCreatedEvent {
    job_id: job.id.clone(),
    spec: job.spec.clone(),
    queued_by: queued_by.clone(),
}).await?;

// 6. Manejador de eventos dispara aprovisionamiento
provisioning_handler.handle(JobCreatedEvent).await?;

// 7. Worker es aprovisionado y ejecuta el trabajo
// 8. JobCompletionEvent publicado
// 9. Estado del trabajo actualizado a Succeeded
```

### Caso de Uso 2: Ejecutar Trabajo con Secretos

**Escenario**: Ejecutar un trabajo que requiere secretos (API keys, tokens)

```bash
# 1. Crear secreto (vía API REST o gRPC)
grpcurl -plaintext -d '{
  "secret": {
    "name": "api-key-prod",
    "value": {"secret_string": "sk-prod-1234567890"},
    "secret_type": "STRING"
  },
  "created_by": "equipo-dev"
}' localhost:50051 hodei.CredentialService.CreateSecret

# 2. Enviar trabajo con referencia a secreto
grpcurl -plaintext -d '{
  "job_definition": {
    "name": "Trabajo API",
    "command": {
      "shell": {
        "cmd": "python",
        "args": ["-c", "import json; import sys; data = json.load(sys.stdin); print(f\"API Key: {data['api-key-prod']}\")"]
      }
    },
    "secrets": ["api-key-prod"]
  },
  "queued_by": "equipo-dev"
}' localhost:50051 hodei.JobExecutionService/QueueJob
```

**Flujo de Inyección de Secretos**:

```rust
// Lado del Worker - JobExecutor
impl JobExecutor {
    async fn execute_with_secrets(&self, job: RunJobCommand) -> Result<JobResult> {
        // 1. Obtener secretos (si hay)
        let secrets = if let Some(secret_names) = &job.secrets {
            self.fetch_secrets(secret_names).await?
        } else {
            HashMap::new()
        };

        // 2. Crear script temporal
        let script_path = self.create_temp_script(&job.command_type).await?;

        // 3. Inyectar secretos vía stdin
        let mut child = Command::new(&job.interpreter)
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Escribir secretos a stdin y cerrar inmediatamente
        if !secrets.is_empty() {
            let json = serde_json::to_string(&secrets)?;
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(json.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        // 4. Stream de logs (secretos son redactados automáticamente)
        self.stream_logs(&mut child, &job.job_id).await?;

        // 5. Esperar completitud
        let status = child.wait().await?;

        Ok(JobResult {
            success: status.success(),
            exit_code: status.code().unwrap_or(-1),
            error_message: None,
        })
    }
}
```

### Caso de Uso 3: Trabajo Programado (Tipo Cron)

**Escenario**: Programar un trabajo para correr cada hora

```bash
# 1. Crear trabajo programado
grpcurl -plaintext -d '{
  "scheduled_job": {
    "name": "Respaldo Horario",
    "cron_expression": "0 * * * *",
    "job_definition": {
      "name": "Respaldo",
      "command": {
        "shell": {
          "cmd": "backup-script.sh",
          "args": []
        }
      }
    },
    "enabled": true
  },
  "created_by": "equipo-ops"
}' localhost:50051 hodei.SchedulerService.CreateScheduledJob

# 2. Listar trabajos programados
grpcurl -plaintext -d '{}' localhost:50051 hodei.SchedulerService.ListScheduledJobs
```

### Caso de Uso 4: Auto-escalado de Workers

**Escenario**: Escalar automáticamente workers basado en longitud de cola

```bash
# Configurar auto-escalado
grpcurl -plaintext -d '{
  "config": {
    "min_workers": 2,
    "max_workers": 10,
    "scale_up_threshold": 5,
    "scale_down_threshold": 2,
    "scale_up_cooldown": 300,
    "scale_down_cooldown": 600
  }
}' localhost:50051 hodei.ProviderManagementService.ConfigureAutoScaling
```

---

## Implementación de Patrones de Diseño

### 1. Patrón Repository

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: abstractar lógica de acceso a datos.
- **OCP**: Abierto para extender con nuevos métodos de repository sin modificar existentes.
- **DIP**: Capas superiores dependen del trait `JobRepository` (abstracción), no de implementaciones concretas.

**🏗️ Ámbito en DDD**: **Capa de Dominio - Definición de Puerto + Adaptador de Infraestructura**
- **Capa de Dominio**: Define el **puerto** (trait `JobRepository`) que especifica el contrato
- **Capa de Infraestructura**: Implementa el **adaptador** (`PostgresJobRepository`) que conecta el puerto con la tecnología
- Separación clara entre: Qué necesito hacer (dominio) vs Cómo lo hago (infraestructura)

**🔗 Dependencias**:
- **Capa de Dominio** (Trait):
  - `Job`, `JobId`, `JobState` (tipos de dominio)
  - `Result<T>` del shared kernel
  - `Send + Sync`, `async_trait` (traits de concurrencia)
  
- **Capa de Infraestructura** (Implementación):
  - Implementa trait `JobRepository`
  - `sqlx` (cliente asíncrono PostgreSQL)
  - `PgPool` (pool de conexiones)
  - `serde_json` (serialización de tipos complejos)

**💡 Motivación**:
Se creó para:
1. **Separar dominio de infraestructura**: El dominio no sabe de SQL, bases de datos, o tecnologías específicas
2. **Facilitar testing**: Puedo usar implementaciones en memoria (`InMemoryJobRepository`) para tests unitarios sin DB
3. **Permitir múltiples implementaciones**: PostgreSQL, MySQL, Redis, en memoria, capa de caché
4. **Seguir DDD**: Repositorios son parte del patrón Repository en Diseño Guiado por el Dominio
5. **Seguridad de Tipos**: SQLx genera queries tipadas en tiempo de compilación

**🎨 Patrones Aplicados**:
- **Patrón Repository**: Abstrae el almacenamiento y recuperación de objetos de dominio
- **Inversión de Dependencia**: El dominio define la interfaz (trait), infraestructura la implementa
- **Puerto y Adaptador**: `JobRepository` es el puerto, `PostgresJobRepository` es el adaptador
- **Active Record vs Repository**: Usamos Repository (no Active Record) para mantener dominio puro
- **Queries con Seguridad de Tipos**: SQLx genera queries verificadas en tiempo de compilación

**⚠️ Consideraciones de Diseño**:
- **Async/Await**: Todos los métodos son asíncronos para no bloquear el runtime
- **Manejo de Errores**: Convierte `sqlx::Error` a `Result<T>` del dominio
- **Pooling de Conexiones**: Usa `PgPool` para reuso eficiente de conexiones
- **Optimización de Queries**: Métodos especializados (`find_by_id`, `find_by_state`) con queries optimizados
- **Upsert**: `ON CONFLICT DO UPDATE` para idempotencia en guardados
- **Gestión de Transacciones**: Repositorios participan en transacciones cuando son necesarias

**🔌 Flujo de Integración**:
```mermaid
graph TB
    subgraph "Capa de Dominio"
        A[Trait JobRepository]
        A -->|Define| B[save/find_by_id/find_by_state]
    end
    
    subgraph "Capa de Aplicación"
        C[CreateJobUseCase]
        D[JobController]
        E[Orquestador Saga]
    end
    
    subgraph "Capa de Infraestructura"
        F[PostgresJobRepository]
        G[InMemoryJobRepository]
        H[RedisJobRepository]
    end
    
    subgraph "Base de Datos"
        I[(PostgreSQL)]
        J[(En-Memoria)]
        K[(Caché Redis)]
    end
    
    C -->|Depende de| A
    D -->|Depende de| A
    E -->|Depende de| A
    
    F -->|Implementa| A
    F -->|Persiste a| I
    G -->|Implementa| A
    G -->|Almacena en| J
    H -->|Implementa| A
    H -->|Caché a| K
```

```rust
// Trait de dominio
#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn save(&self, job: &Job) -> Result<()>;
    async fn find_by_id(&self, id: &JobId) -> Result<Option<Job>>;
    async fn find_by_state(&self, state: &JobState) -> Result<Vec<Job>>;
}

// Implementación de infraestructura
pub struct PostgresJobRepository {
    pool: PgPool,
}

#[async_trait]
impl JobRepository for PostgresJobRepository {
    async fn save(&self, job: &Job) -> Result<()> {
        sqlx::query!(
            "INSERT INTO jobs (id, spec, state, ...) VALUES ($1, $2, $3, ...)",
            job.id.as_uuid(),
            serde_json::to_value(&job.spec)?,
            job.state as i32,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

---

### 2. Patrón Estrategia

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada estrategia de planificación tiene una única responsabilidad: calcular un puntaje para un worker.
- **OCP**: Abierto para agregar nuevas estrategias de planificación sin modificar el código existente.
- **DIP**: El planificador depende del trait `SchedulingStrategy`, no de implementaciones concretas.
- **Interfaz de Estrategia**: Define un contrato que todas las estrategias deben cumplir.

**🏗️ Ámbito en DDD**: **Capa de Dominio - Contexto de Planificación**
- Define el **puerto** (`SchedulingStrategy`) para algoritmos de asignación de jobs a workers
- Las estrategias implementan el **adaptador** que traduce reglas de negocio a puntajes numéricos
- Permite cambiar la estrategia de planificación sin afectar otros componentes

**🔗 Dependencias**:
- **Capa de Dominio** (Trait):
  - `Job`, `Worker`, `ProviderConfig` (tipos de dominio)
  - `f64` (tipo de retorno para puntaje)
  - `Send + Sync` (seguridad de hilos)
  
- **Capa de Dominio** (Implementaciones):
  - `LeastLoadedStrategy`: Depende de `Worker.resources`, `ProviderConfig`
  - `ResourceAwareStrategy`: Depende de `Job.spec.resources`, `Worker.spec.resources`
  - `CostOptimizedStrategy`: Depende de métricas de costo

**💡 Motivación**:
Se creó para:
1. **Encapsular algoritmos de planificación**: Cada estrategia es un algoritmo autónomo
2. **Facilitar extensión**: Agregar nuevas estrategias sin modificar código existente
3. **Testing**: Cada estrategia puede testearse aisladamente
4. **Configuración en Tiempo de Ejecución**: Cambiar estrategia sin recompilar (archivo config / var entorno)
5. **Comparación**: Facilitar pruebas A/B de diferentes estrategias

**🎨 Patrones Aplicados**:
- **Patrón Estrategia**: Encapsula algoritmos intercambiables
- **Patrón Composición**: Múltiples estrategias pueden componerse (cadena de responsabilidad)
- **Selección Basada en Puntaje**: Todos los algoritmos retornan un puntaje normalizado (0.0-1.0)
- **Decisión Ponderada**: Puntaje final puede ser combinación ponderada de múltiples factores
- **Abstracción de Algoritmo**: La interfaz es agnóstica al algoritmo específico

**⚠️ Consideraciones de Diseño**:
- **Normalización de Puntaje**: Todas las estrategias deben retornar puntajes en el mismo rango
- **Determinista vs Aleatorio**: Estrategias pueden ser deterministas o incluir aleatoriedad
- **Conciencia de Contexto**: Estrategias pueden usar requerimientos de job, estado de worker, capacidad de proveedor
- **Estrategia de Respaldo**: Debe existir una estrategia por defecto
- **Rendimiento**: Cálculo de puntaje debe ser rápido (no hacer E/S, solo cálculos en memoria)

**📊 Comparación de Estrategias de Planificación**:
| Estrategia | Fórmula | Caso de Uso | Rango de Puntaje |
|-------------|----------|----------|-------------|
| **LeastLoaded** | 1.0 - (uso_cpu + uso_mem) / 2 | Balanceo de carga general | 0.0 - 1.0 |
| **ResourceAware** | puntaje_ajuste = min(ajuste_cpu, ajuste_mem) | Jobs con requerimientos específicos | 0.0 - 1.0 |
| **RoundRobin** | Ciclo secuencial | Simple y determinista | N/A |
| **PriorityBased** | job.prioridad + worker.capacidad | Jobs críticos | 0.0 - 2.0 |
| **CostOptimized** | 1.0 - (costo / costo_max) | Cargas de trabajo sensibles al costo | 0.0 - 1.0 |

**🔌 Flujo de Selección de Estrategia**:
```mermaid
graph TB
    A[Planificador] -->|Seleccionar Estrategia| B{Configuración}
    
    B -->|Menos Cargado| C[LeastLoadedStrategy]
    B -->|Consciente de Recursos| D[ResourceAwareStrategy]
    B -->|Optimizado por Costo| E[CostOptimizedStrategy]
    
    C --> F[Para cada worker disponible]
    D --> F
    E --> F
    
    F --> G[Calcular puntaje]
    G --> H{Seleccionar mejor puntaje}
    
    H -->|Worker encontrado| I[Retornar WorkerId]
    H -->|Ningún worker| J[Retornar None]
    
    A -->|Asignar job| I
```

```rust
pub trait SchedulingStrategy: Send + Sync {
    fn score(&self, job: &Job, worker: &Worker, provider: &ProviderConfig) -> f64;
}

pub struct LeastLoadedStrategy;

impl SchedulingStrategy for LeastLoadedStrategy {
    fn score(&self, job: &Job, worker: &Worker, provider: &ProviderConfig) -> f64 {
        let cpu_score = 1.0 - (worker.resources.cpu_usage_percent / 100.0);
        let memory_score = 1.0 - (worker.resources.memory_usage_percent / 100.0);
        (cpu_score + memory_score) / 2.0
    }
}
```

---

### 3. Patrón Observador (Eventos de Dominio)

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada manejador de eventos tiene una única responsabilidad: reaccionar a un tipo de evento específico.
- **OCP**: Abierto para agregar nuevos manejadores sin modificar código existente.
- **DIP**: El sistema depende del trait `EventHandler`, no de implementaciones concretas.
- **Desacoplamiento**: Productores (agregados) no dependen de consumidores (manejadores).

**🏗️ Ámbito en DDD**: **Capa de Dominio - Bus de Eventos + Capa de Aplicación**
- **Capa de Dominio**: Define eventos como hechos que ocurrieron en el dominio
- **Bus de Eventos**: Componente que orquesta publicación y consumo de eventos
- **Capa de Aplicación**: Implementa manejadores que reaccionan a eventos
- Implementa **Arquitectura Orientada a Eventos** para comunicación eventualmente consistente

**🔗 Dependencias**:
- **Capa de Dominio** (Eventos):
  - `JobCompletedEvent`, `WorkerRegisteredEvent`, etc. (structs de eventos)
  - `DomainEvent` trait (trait marcador)
  - `EventHandler<E>` trait (interfaz de manejador)
  
- **Capa de Aplicación** (Manejadores):
  - `JobCompletionNotificationHandler`: Depende de `NotificationService`
  - `WorkerProvisioningHandler`: Depende de `WorkerProvisioningService`
  - `AuditEventHandler`: Depende de `AuditLogRepository`

**💡 Motivación**:
Se creó para:
1. **Desacoplar contextos delimitados**: Jobs, Workers, Planificación comunican vía eventos, no llamadas directas
2. **Consistencia Eventual**: Permite comunicación eventualmente consistente entre contextos
3. **Auditoría**: Todos los cambios importantes se registran como eventos
4. **Extensibilidad**: Agregar nuevas reacciones a eventos sin modificar código existente
5. **Sistema Reactivo**: El sistema reacciona a eventos en tiempo real

**🎨 Patrones Aplicados**:
- **Patrón Observador**: Productores notifican a observadores (manejadores) sobre cambios
- **Bus de Eventos**: Centraliza la publicación y consumo de eventos
- **Pub/Sub**: Múltiples consumidores pueden suscribir al mismo evento
- **Eventos con Seguridad de Tipos**: Enums y structs tipados aseguran que solo se manejen eventos válidos
- **Manejo de Eventos Asíncrono**: Manejadores son asíncronos para no bloquear el bus de eventos

**⚠️ Consideraciones de Diseño**:
- **Orden de Eventos**: El orden de procesamiento de eventos no está garantizado entre manejadores
- **Idempotencia**: Manejadores deben ser idempotentes (mismo evento puede procesarse múltiples veces)
- **Manejo de Errores**: Error en un manejador no debe afectar otros manejadores
- **Garantías de Entrega**: Al-Menos-Una-Vez (puede haber duplicados, debe ser idempotente)
- **Versionado de Eventos**: Eventos deben versionarse para compatibilidad hacia atrás
- **Cola de Mensajes Muertos (DLQ)**: Eventos que fallan múltiples veces deberían ir a DLQ

**🔌 Flujo de Eventos**:
```mermaid
graph LR
    subgraph "Productor"
        A[Agregado Job]
        A -->|Publica| B[JobCompletedEvent]
    end
    
    subgraph "Bus de Eventos"
        C[EventBus]
        C -->|Despacha a| D[Manejador 1]
        C -->|Despacha a| E[Manejador 2]
        C -->|Despacha a| F[Manejador 3]
    end
    
    subgraph "Consumidores"
        D -->|Maneja| G[Servicio Notificación]
        E -->|Maneja| H[Servicio Auditoría]
        F -->|Maneja| I[Servicio Métricas]
    end
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCompletedEvent {
    pub job_id: JobId,
    pub result: JobExecutionResult,
    pub completed_at: DateTime<Utc>,
}

pub struct JobCompletionNotificationHandler {
    notification_service: Arc<NotificationService>,
}

#[async_trait]
impl EventHandler<JobCompletedEvent> for JobCompletionNotificationHandler {
    async fn handle(&self, event: &JobCompletedEvent) -> Result<()> {
        let job = self.job_repo.find_by_id(&event.job_id).await?;
        if job.spec.preferences.notify_on_completion {
            self.notification_service.send(&job.queued_by, &format!("Job {} completado", job.name)).await?;
        }
        Ok(())
    }
}
```

---

### 4. Patrón Saga

**📋 Responsabilidad (SOLID)**:
- **SRP**: Cada paso de saga tiene una única responsabilidad: ejecutar una acción transaccional o compensarla.
- **OCP**: Abierto para agregar nuevos pasos sin modificar el orquestador.
- **DIP**: El orquestador de saga depende del trait `SagaStep`, no de implementaciones concretas.
- **Compensación**: Cada paso define su propia lógica de compensación.

**🏗️ Ámbito en DDD**: **Capa de Aplicación - Orquestación Saga**
- **Orquestador**: `ProvisioningSaga` coordina múltiples pasos transaccionales
- **Pasos**: Cada paso encapsula una acción que puede compensarse
- **Datos de Compensación**: Almacena información necesaria para rollback
- Implementa **Patrón Saga** para transacciones distribuidas

**🔗 Dependencias**:
- **Capa de Dominio**:
  - `SagaStep` trait (define ejecutar/compensar)
  - `Saga` struct (estado del saga)
  
- **Capa de Aplicación** (Pasos Saga):
  - `GenerateOTPStep`: Depende de `OtpTokenStore`
  - `ProvisionWorkerStep`: Depende de `WorkerProvider`
  - `WaitForRegistrationStep`: Depende de `WorkerRegistry`

**💡 Motivación**:
Se creó para:
1. **Transacciones Distribuidas**: Coordinar múltiples servicios manteniendo consistencia eventual
2. **Compensación en Fallo**: Si un paso falla, compensar todos los pasos anteriores
3. **Recuperación**: Permitir reanudar sagas interrumpidas (durante caída del servidor)
4. **Trazabilidad**: Cada paso tiene datos de compensación para rollback auditado
5. **Idempotencia**: Los pasos deben ser idempotentes para permitir reintentos

**🎨 Patrones Aplicados**:
- **Patrón Saga**: Orquesta transacciones distribuidas con compensación
- **Transacción Compensatoria**: Si falla, ejecutar compensaciones en orden inverso
- **Máquina de Estados**: Saga tiene estados (Pendiente, EnProgreso, Compensando, Completado, Fallido)
- **Checkpointing**: Almacena datos de compensación por cada paso completado
- **Orquestación**: Orquestador Saga coordina el flujo de pasos

**⚠️ Consideraciones de Diseño**:
- **Límites Transaccionales**: Cada paso debe ser transaccional en su propio servicio
- **Semántica de Compensación**: Compensación no debe fallar (mejor esfuerzo)
- **Manejo de Timeout**: Cada paso debe tener timeout para evitar operaciones colgadas
- **Estrategia de Reintento**: Implementar reintento con backoff exponencial para pasos que fallan
- **Persistencia de Saga**: El estado del saga debe persistirse para recuperación
- **Sagas Concurrentes**: Múltiples sagas pueden ejecutarse concurrentemente

**📊 Ciclo de Vida Saga**:
```
Pendiente → EnProgreso → Completado
         ↓
    Compensando → Fallido
```

**🔌 Flujo Saga**:
```mermaid
graph TB
    A[Orquestador Saga] -->|Ejecutar| B[Paso 1: Generar OTP]
    B -->|Éxito| C[Guardar datos compensación]
    B -->|Error| D[Compensar: Sin pasos previos]
    
    C -->|Ejecutar| E[Paso 2: Aprovisionar Worker]
    E -->|Éxito| F[Guardar datos compensación]
    E -->|Error| G[Compensar Paso 1]
    
    F -->|Ejecutar| H[Paso 3: Esperar Registro]
    H -->|Éxito| I[Saga Completada]
    H -->|Error| J[Compensar Pasos 2, 1]
    
    G -->|Compensación fallida| K[Saga Fallida]
    J -->|Todas compensaciones exitosas| K
```

```rust
pub struct ProvisioningSaga {
    steps: Vec<Box<dyn SagaStep>>,
    current_step: usize,
    compensation_data: HashMap<String, serde_json::Value>,
}

impl ProvisioningSaga {
    pub async fn execute(&mut self) -> Result<()> {
        for step in &self.steps {
            if let Err(e) = step.execute(self.get_input(step)?).await {
                self.compensate(self.current_step).await?;
                return Err(e);
            }
            self.current_step += 1;
        }
        Ok(())
    }

    async fn compensate(&mut self, from_step: usize) -> Result<()> {
        for step in (0..from_step).rev() {
            let input = self.get_compensating_input(step)?;
            self.steps[step].compensate(input).await?;
        }
        Ok(())
    }
}
```

---

## Monitoreo y Observabilidad

### Recolección de Métricas

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: recolectar y exponer métricas del sistema.
- **OCP**: Abierto para agregar nuevas métricas sin modificar la estructura principal.
- **DIP**: No depende de implementaciones concretas de exportación (Prometheus, StatsD, etc.).

**🏗️ Ámbito en DDD**: **Capa de Infraestructura - Observabilidad**
- Implementa **observabilidad** del sistema a través de métricas
- Proporciona **telemetría** para monitoreo, alertas, depuración
- Separa **métricas de dominio** (jobs completados, workers activos) de **métricas de infraestructura** (tamaño pool db, tasa aciertos caché)

**🔗 Dependencias**:
- **Externa**: librería cliente `prometheus`
- **Dominio**: Tipos de dominio para métricas de negocio
- **Concurrencia**: Contadores y medidores seguros para hilos

**💡 Motivación**:
Se creó para:
1. **Observabilidad**: Permitir monitoreo del sistema en producción
2. **Alertas**: Facilitar alertas basadas en métricas
3. **Seguimiento de Rendimiento**: Monitorear latencias y rendimiento
4. **Planificación de Capacidad**: Basar decisiones de escalado en métricas
5. **Depuración**: Ayudar a identificar problemas en producción

**🎨 Patrones Aplicados**:
- **Patrón Métricas**: Contadores, Medidores, Histogramas para diferentes tipos de métricas
- **Push vs Pull**: Prometheus usa pull (scraping), otros sistemas usan push
- **Nombramiento de Métricas**: Convención de namespace (`jobs_submitted_total`, `workers_active_count`)
- **Histogramas**: Para métricas de distribución (latencia)
- **Contadores de Tasa**: Para métricas de tasa (solicitudes por segundo)

**⚠️ Consideraciones de Diseño**:
- **Cardinalidad de Métricas**: Evitar etiquetas de alta cardinalidad
- **Granularidad de Métricas**: Balance entre granularidad y sobrecarga
- **Persistencia de Métricas**: Prometheus guarda series temporales
- **Muestreo**: Histogramas usan muestreo para eficiencia de memoria
- **Estrategia de Etiquetas**: Etiquetas dimensionales vs métricas específicas

```rust
use prometheus::{Counter, Histogram, Gauge};

pub struct Metrics {
    jobs_submitted: Counter,
    jobs_completed: Counter,
    jobs_failed: Counter,
    job_duration: Histogram,
    workers_active: Gauge,
}

impl Metrics {
    pub fn record_job_completed(&self, duration: Duration) {
        self.jobs_completed.inc();
        self.job_duration.observe(duration.as_secs_f64());
    }

    pub fn update_workers_active(&self, count: usize) {
        self.workers_active.set(count as f64);
    }
}
```

---

### Trazado Distribuido

**📋 Responsabilidad (SOLID)**:
- **SRP**: Responsabilidad única: capturar y propagar trazas distribuidas.
- **OCP**: Abierto para cambiar exportador de trazas (Jaeger, Zipkin, OTLP) sin modificar código.
- **DIP**: Depende de abstracciones de OpenTelemetry, no de exportadores específicos.

**🏗️ Ámbito en DDD**: **Capa de Infraestructura - Observabilidad**
- Implementa **trazado distribuido** para entender el flujo de solicitudes
- Proporciona **visibilidad de extremo a extremo** a través de múltiples servicios
- Facilita **análisis de causa raíz** en producción

**🔗 Dependencias**:
- **Externa**: `opentelemetry`, `opentelemetry-jaeger`, `tracing-opentelemetry`
- **Dominio**: Metadatos de Span (job_id, worker_id, etc.)
- **Concurrencia**: Propagación de contexto asíncrono

**💡 Motivación**:
Se creó para:
1. **Depuración Distribuida**: Entender el flujo de solicitudes entre servicios
2. **Análisis de Rendimiento**: Identificar cuellos de botella en la tubería
3. **Análisis de Causa Raíz**: Diagnosticar problemas en producción
4. **Dependencias de Servicio**: Visualizar dependencias entre servicios
5. **Correlación de Solicitudes**: Correlacionar logs de una misma solicitud

**🎨 Patrones Aplicados**:
- **Patrón Trazado Distribuido**: Spans anidados para cadenas de solicitud
- **Propagación de Contexto**: Trace ID y span ID propagados entre servicios
- **Contexto de Traza W3C**: Compatibilidad con estándar W3C
- **Equipaje (Baggage)**: Metadatos propagados a través de todos los spans hijos
- **Atributos de Span**: Metadatos estructurados adjuntos a spans

**⚠️ Consideraciones de Diseño**:
- **Muestreo**: Implementar muestreo para escenarios de alto tráfico
- **Granularidad de Span**: Balance entre granularidad y sobrecarga
- **Cardinalidad de Atributos**: Evitar atributos de alta cardinalidad
- **Datos Sensibles**: Nunca incluir secretos o PII en spans
- **Formato de Exportación**: OTLP (Protocolo OpenTelemetry) para interoperabilidad

```rust
use opentelemetry::trace::{Tracer, TraceError};
use tracing_opentelemetry::OpenTelemetryLayer;

fn init_tracing() -> Result<TraceError> {
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name("hodei-server")
        .install_simple()?;

    let telemetry = OpenTelemetryLayer::new(tracer);
    tracing_subscriber::registry().with(telemetry).try_init()
}
```

---

## Guía de Solución de Problemas

### Problemas Comunes

#### 1. Worker No Registrándose

**Diagnóstico**:
```bash
grep "GenerateOTP" /var/log/hodei-server.log
grep "Register" /var/log/hodei-worker.log
docker exec <worker_container> cat /var/run/hodei/otp.txt
```

**Soluciones**:
- Incrementar TTL de token OTP: `HODEI_OTP_TTL_SECS=600`
- Verificar conectividad de red
- Verificar validez de certificado mTLS

#### 2. Jobs Atascados en Estado "Queued"

**Diagnóstico**:
```bash
grpcurl -plaintext -d '{}' localhost:50051 hodei.SchedulerService.GetQueueStatus
grpcurl -plaintext -d '{}' localhost:50051 hodei.SchedulerService.GetAvailableWorkers
```

**Soluciones**:
- Verificar que auto-escalado esté habilitado
- Verificar salud del proveedor
- Asegurar que imágenes de worker estén disponibles

#### 3. Alto Uso de Memoria en Worker

**Diagnóstico**:
```bash
grpcurl -plaintext -d '{}' localhost:50051 hodei.MetricsService.GetWorkerMetrics
docker stats
```

**Soluciones**:
- Reducir tamaño de lote de job
- Implementar truncamiento de logs
- Agregar límites de memoria al contenedor

---

## Mejores Prácticas

### 1. Diseño de Job

**HACER**:
- Mantener jobs sin estado e idempotentes
- Usar límites de recursos apropiados
- Implementar apagado elegante
- Establecer timeouts razonables

**NO HACER**:
- Almacenar estado en entorno del job
- Asumir que dependencias externas están disponibles
- Ignorar condiciones de error
- Crear bucles infinitos

### 2. Manejo de Errores

**HACER**:
- Usar tipos Result explícitamente
- Proveer contexto con errores
- Loguear errores con nivel apropiado
- Implementar lógica de reintento para errores transitorios

**NO HACER**:
- Usar unwrap() en código de producción
- Ignorar errores
- Entrar en pánico en errores recuperables
- Loguear contraseñas o secretos

### 3. Optimización de Rendimiento

**HACER**:
- Usar async/await para operaciones de E/S
- Operaciones de base de datos por lotes
- Usar pooling de conexiones
- Implementar caché donde sea apropiado

**NO HACER**:
- Bloquear runtime asíncrono
- Patrones de consulta N+1
- Asignar innecesariamente
- Deshabilitar optimizaciones prematuramente

---

## Guía de Extensión

### Agregando un Nuevo Proveedor de Worker

**Paso 1**: Implementar trait WorkerProvider
**Paso 2**: Agregar variante de configuración
**Paso 3**: Registrar proveedor en fábrica
**Paso 4**: Agregar pruebas exhaustivas

### Agregando un Nuevo Evento de Dominio

**Paso 1**: Definir struct de evento
**Paso 2**: Implementar trait DomainEvent
**Paso 3**: Publicar desde casos de uso
**Paso 4**: Crear manejadores de eventos
**Paso 5**: Registrar manejadores en bus de eventos

---

## Guía de Migración

### Migrando de v7.x a v8.0

**Cambios Rompedores**:

1. Batching de logs habilitado por defecto
2. mTLS requerido para comunicación de worker
3. E/S Zero-copy habilitada

**Pasos de Migración**:

1. Respaldar base de datos
2. Ejecutar migraciones
3. Generar certificados
4. Actualizar configuración
5. Reconstruir workers
6. Verificar despliegue

---

## Glosario

### Términos de Dominio

| Término | Definición |
|------|------------|
| **Agregado** | Grupo de objetos de dominio tratados como una sola unidad |
| **Contexto Delimitado** | Parte específica de la lógica de dominio actuando independientemente |
| **Evento de Dominio** | Algo importante en el dominio para otras partes del sistema |
| **Objeto de Valor** | Objeto inmutable definido por sus atributos |
| **Repositorio** | Interfaz tipo colección para acceder a objetos de dominio |
| **Saga** | Secuencia de transacciones locales para operaciones distribuidas |
| **CQRS** | Patrón de Segregación de Responsabilidad de Comando y Consulta |
| **Patrón Outbox** | Patrón para garantizar publicación de eventos en transacciones |

### Términos Técnicos

| Término | Definición |
|------|------------|
| **mTLS** | Autenticación TLS Mutua |
| **OTP** | Contraseña de Un Solo Uso |
| **gRPC** | Framework de Llamada a Procedimiento Remoto |
| **E/S Zero-Copy** | E/S sin copia de datos |
| **Contrapresión (Backpressure)** | Mecanismo para manejar control de flujo |
| **DDD** | Diseño Guiado por el Dominio |
| **SOLID** | Cinco principios de diseño |

---

## Conclusión

Este documento de arquitectura proporciona una visión completa del diseño de la Plataforma Hodei Jobs, cubriendo:

1. **Diseño Guiado por el Dominio**: 9 contextos delimitados con responsabilidades claras
2. **Arquitectura Hexagonal**: Separación limpia de preocupaciones
3. **Diseño Orientado a Eventos**: Sistema reactivo con eventos de dominio
4. **Seguridad Zero Trust**: Autenticación mTLS + OTP
5. **Optimizaciones de Rendimiento**: LogBatching, Zero-Copy, Métricas en Caché
6. **Soporte Multi-Proveedor**: Docker, Kubernetes, Firecracker
7. **Listo para Producción**: Pruebas exhaustivas, monitoreo, despliegue

La plataforma está diseñada para **escalabilidad, confiabilidad y seguridad**, permitiendo a los equipos ejecutar cargas de trabajo a través de cualquier infraestructura con experiencia operativa consistente.

---

**Versión del Documento**: 1.0  
**Última Actualización**: 2026-01-09  
**Mantenido Por**: Equipo de Arquitectura  
**Ciclo de Revisión**: Trimestral
