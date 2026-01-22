# Deuda Técnica: Violaciones de SOLID, DDD y Connascence

**Fecha**: 2026-01-22  
**Estado**: Activo  
**Prioridad**: Alta  
**Épica Relacionada**: EPIC-93 - Saga Engine v4 Event Sourcing

---

## Índice

1. [Resumen Ejecutivo](#resumen-ejecutivo)
2. [Violaciones de Interface Segregation Principle (ISP)](#violaciones-de-isp)
3. [Violaciones de Dependency Inversion Principle (DIP)](#violaciones-de-dip)
4. [Violaciones de Single Responsibility Principle (SRP)](#violaciones-de-srp)
5. [Violaciones de Open/Closed Principle (OCP)](#violaciones-de-ocp)
6. [Violaciones de Liskov Substitution Principle (LSP)](#violaciones-de-lsp)
7. [Violaciones de DDD](#violaciones-de-ddd)
8. [Problemas de Connascence](#problemas-de-connascence)
9. [Plan de Refactorización Priorizado](#plan-de-refactorización-priorizado)

---

## Resumen Ejecutivo

Se han identificado **23 violaciones significativas** a principios SOLID, patrones DDD y problemas de connascence en el código base de Hodei Jobs. Estas violaciones generan:

- **Acoplamiento fuerte** entre capas arquitectónicas
- **Dificultad para testing** debido a dependencias concretas
- **Código duplicado** entre traits similares
- **Resistencia al cambio** por violaciones de OCP

`★ Insight ─────────────────────────────────────`
- **Impacto acumulativo**: Estas violaciones no son aisladas; se refuerzan mutuamente. Por ejemplo, violaciones de ISP causan violaciones de DIP cuando los clientes necesitan implementar interfaces completas.
- **Deuda técnica progresiva**: Cada nueva funcionalidad agregada sobre estas violaciones incrementa exponencialmente el costo de mantenimiento.
- **Oportunidad estratégica**: EPIC-93 (Saga Engine v4) es el momento ideal para abordar esta deuda mientras se refactoriza la arquitectura de saga.
`─────────────────────────────────────────────────`

---

## Violaciones de ISP

### DEBT-001: WorkerProvider como "God Trait"

**Archivo**: `crates/server/domain/src/workers/provider_api.rs:748-756`

**Descripción**:
```rust
pub trait WorkerProvider:
    WorkerProviderIdentity
    + WorkerLifecycle
    + WorkerLogs
    + WorkerCost
    + WorkerHealth
    + WorkerEligibility
    + WorkerMetrics
    + WorkerEventSource
    + Send
    + Sync
{
}
```

**Problema**:
- **Connascence de Nombre (CoN)**: Clientes deben conocer 8 sub-traits diferentes
- **Violación de ISP**: Clientes que solo necesitan health check dependen de logs, cost, metrics
- **Acoplamiento temporal**: Cambios en cualquier sub-trait afectan a todos los implementadores

**Impacto**:
- Testing requiere mocks de 8 traits aunque solo use 1 método
- Nuevos providers deben implementar ~30 métodos
- Imposible crear `dyn WorkerProvider` sin problemas de object safety

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Composición sobre Herencia =====

// 1. Core trait - Solo operaciones esenciales
#[async_trait]
pub trait WorkerProviderCore: Send + Sync {
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<()>;
}

// 2. Optional capabilities - Traits independientes
#[async_trait]
pub trait WorkerProviderHealth: Send + Sync {
    async fn health_check(&self) -> Result<HealthStatus>;
}

#[async_trait]
pub trait WorkerProviderLogs: Send + Sync {
    async fn get_worker_logs(&self, handle: &WorkerHandle, tail: Option<u32>) 
        -> Result<Vec<LogEntry>>;
}

// 3. Builder pattern para providers que necesitan funcionalidad completa
pub struct FullWorkerProvider {
    core: Arc<dyn WorkerProviderCore>,
    health: Option<Arc<dyn WorkerProviderHealth>>,
    logs: Option<Arc<dyn WorkerProviderLogs>>,
    // ... otras capacidades opcionales
}

impl FullWorkerProvider {
    pub fn builder() -> WorkerProviderBuilder {
        WorkerProviderBuilder::new()
    }
}

// 4. Uso en código de aplicación - Solo dependencia de lo necesario
struct SagaWorkerProvisioner {
    // Solo necesita crear y destruir
    provider: Arc<dyn WorkerProviderCore>,
}

struct MonitoringService {
    // Solo necesita health checks
    providers: Vec<Arc<dyn WorkerProviderHealth>>,
}
```

**Beneficios**:
- Transforma **Connascence of Name** → **Connascence of Type** (más débil)
- Testing: Mock simple con solo métodos necesarios
- Providers opcionales pueden implementar solo lo que necesitan
- Compatible con `dyn Trait` para runtime polymorphism

**Esfuerzo**: 3-4 días  
**Prioridad**: ALTA

---

### DEBT-002: WorkerProvisioningService con Múltiples Responsabilidades

**Archivo**: `crates/server/application/src/workers/provisioning.rs`

**Descripción**:
El trait `WorkerProvisioningService` mezcla:
- Operaciones de aprovisionamiento (`provision_worker`, `destroy_worker`)
- Consultas (`list_providers`, `default_worker_spec`)
- Configuración (`get_provider_config`)
- Validación (`validate_spec`)

**Problema**:
```rust
// Cliente que solo necesita listar providers debe conocer aprovisionamiento
struct ProviderCatalog {
    provisioning: Arc<dyn WorkerProvisioningService>, // <- Demasiado grande
}
```

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Segregación por Rol =====

// 1. Rol: Aprovisionamiento (para sagas)
#[async_trait]
pub trait WorkerProvisioner: Send + Sync {
    async fn provision_worker(&self, provider_id: &ProviderId, spec: WorkerSpec, job_id: JobId) 
        -> Result<WorkerProvisioningResult>;
    async fn destroy_worker(&self, worker_id: &WorkerId) -> Result<()>;
    async fn terminate_worker(&self, worker_id: &WorkerId, reason: &str) -> Result<()>;
}

// 2. Rol: Consulta de providers (para UI/API)
#[async_trait]
pub trait ProviderCatalog: Send + Sync {
    async fn list_providers(&self) -> Result<Vec<ProviderId>>;
    async fn is_provider_available(&self, provider_id: &ProviderId) -> Result<bool>;
    async fn default_worker_spec(&self, provider_id: &ProviderId) -> Option<WorkerSpec>;
}

// 3. Rol: Configuración (para admin)
#[async_trait]
pub trait ProviderConfigurator: Send + Sync {
    async fn get_provider_config(&self, provider_id: &ProviderId) 
        -> Result<Option<ProviderConfig>>;
    async fn validate_spec(&self, spec: &WorkerSpec) -> Result<()>;
}

// 4. Implementación compuesta (para backward compatibility)
pub struct DefaultWorkerProvisioningService {
    provisioner: Arc<dyn WorkerProvisioner>,
    catalog: Arc<dyn ProviderCatalog>,
    configurator: Arc<dyn ProviderConfigurator>,
}

// Implementa los tres roles por delegación
#[async_trait]
impl WorkerProvisioner for DefaultWorkerProvisioningService {
    async fn provision_worker(...) -> Result<WorkerProvisioningResult> {
        self.provisioner.provision_worker(...).await
    }
    // ... otros métodos
}
```

**Beneficios**:
- Cada cliente depende solo de lo que usa
- Transforma **CoN (Connascence of Name)** → **CoT (Connascence of Type)**
- Testing más granular
- Cumple **Principle of Least Knowledge**

**Esfuerzo**: 2 días  
**Prioridad**: MEDIA

---

### DEBT-003: SagaContext con Demasiadas Responsabilidades

**Archivo**: `crates/server/domain/src/saga/types.rs:153-193`

**Descripción**:
`SagaContext` maneja:
- Metadata de ejecución
- Outputs de steps
- Servicios inyectados
- Estado de saga
- Distributed tracing
- Optimistic locking

**Problema**: Violación de SRP dentro de un struct supposed to be simple

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Context Objects Pattern =====

// 1. Core context - Solo datos de ejecución
#[derive(Clone)]
pub struct SagaContext {
    pub saga_id: SagaId,
    pub saga_type: SagaType,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub started_at: DateTime<Utc>,
    pub current_step: usize,
    pub state: SagaState,
    pub version: u64,
    pub trace_parent: Option<String>,
}

// 2. Step outputs - Separado para compensation
#[derive(Clone, Default)]
pub struct StepOutputs {
    outputs: HashMap<String, serde_json::Value>,
}

impl StepOutputs {
    pub fn set<V: Serialize>(&mut self, step: &str, output: &V) -> SagaResult<()>;
    pub fn get<V: for<'de> Deserialize<'de>>(&self, step: &str) -> Option<V>;
}

// 3. Runtime services - No persistido
#[derive(Clone)]
pub struct SagaServices {
    pub provider_registry: Arc<dyn WorkerRegistryTx + Send + Sync>,
    pub event_bus: Arc<dyn EventBus + Send + Sync>,
    pub job_repository: Option<Arc<dyn JobRepositoryTx + Send + Sync>>,
    pub provisioning_service: Option<Arc<dyn WorkerProvisioning + Send + Sync>>,
    pub orchestrator: Option<Arc<dyn SagaOrchestrator>>,
    pub command_bus: Option<DynCommandBus>,
}

// 4. Metadata - Para datos custom de saga
#[derive(Clone, Default)]
pub struct SagaMetadata {
    data: HashMap<String, serde_json::Value>,
}

// 5. Composición en SagaExecution
pub struct SagaExecution {
    context: SagaContext,
    outputs: StepOutputs,
    metadata: SagaMetadata,
    services: Arc<SagaServices>,
}
```

**Beneficios**:
- Cada componente tiene responsabilidad única
- **CoP (Connascence of Position)** → **CoT (Connascence of Type)**
- Persistencia más clara (solo SagaContext)
- Testing más fácil (mock de servicios separado)

**Esfuerzo**: 2 días  
**Prioridad**: MEDIA

---

## Violaciones de DIP

### DEBT-004: ComandBus Concretos en Dominio

**Archivo**: `crates/server/domain/src/command/bus.rs`

**Descripción**:
El dominio define `InMemoryCommandBus` como implementación concreta

**Problema**:
```rust
// Infraestructura depende de concreto
let bus = InMemoryCommandBus::new(); // <- DIP violado
```

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Abstracción en dominio =====

// 1. Dominio define el contrato
#[async_trait]
pub trait CommandBus: Send + Sync {
    async fn dispatch<C: Command>(&self, command: C) -> Result<C::Output>;
}

// 2. Type alias para trait object (evita E0038)
pub type DynCommandBus = Arc<dyn CommandBus + Send + Sync>;

// 3. Implementaciones en infraestructura
pub struct InMemoryCommandBus { /* ... */ }
pub struct NatsCommandBus { /* ... */ }
pub struct KafkaCommandBus { /* ... */ }

#[async_trait]
impl CommandBus for InMemoryCommandBus { /* ... */ }
#[async_trait]
impl CommandBus for NatsCommandBus { /* ... */ }

// 4. Factory para crear instancias
pub enum CommandBusType {
    InMemory,
    Nats { url: String },
    Kafka { brokers: Vec<String> },
}

impl CommandBusType {
    pub fn create(&self) -> DynCommandBus {
        match self {
            CommandBusType::InMemory => Arc::new(InMemoryCommandBus::new()),
            CommandBusType::Nats { url } => Arc::new(NatsCommandBus::new(url)),
            // ...
        }
    }
}
```

**Beneficios**:
- Dominio no depende de implementaciones concretas
- Testing con mock fácil
- Runtime polymorphism sin casts

**Esfuerzo**: 1 día  
**Prioridad**: ALTA

---

### DEBT-005: PgPool en Application Layer

**Archivo**: Varios archivos en `application/`

**Descripción**:
Se usa `PgPool` directamente en lugar de repository abstractions

**Problema**:
```rust
// Application layer不应该知道数据库细节
struct SomeService {
    pool: PgPool, // <- DIP violado
}
```

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Repository Pattern =====

// 1. Dominio define repositorios
#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn save(&self, job: &Job) -> Result<()>;
    async fn find_by_id(&self, id: &JobId) -> Result<Option<Job>>;
}

// 2. Implementaciones en infraestructura
pub struct PostgresJobRepository {
    pool: PgPool,
}

#[async_trait]
impl JobRepository for PostgresJobRepository {
    async fn save(&self, job: &Job) -> Result<()> {
        // Implementación con pgpool
    }
}

// 3. Application layer usa abstracción
struct JobService {
    repo: Arc<dyn JobRepository>, // <- DIP cumplido
}
```

**Esuerzo**: 3 días  
**Prioridad**: ALTA

---

### DEBT-006: CommandBusJobExecutionPort con Dependencia Concreta

**Archivo**: `crates/server/application/src/saga/bridge/job_execution_port.rs:42-57`

**Descripción**:
Depende directamente de `DynCommandBus` sin abstracción

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Port/Adapter Pattern =====

// 1. Port definido por el workflow (application)
#[async_trait]
pub trait JobExecutionPort: Send + Sync {
    async fn validate_job(&self, job_id: &str) -> Result<bool, String>;
    async fn dispatch_job(&self, job_id: &str, worker_id: &str, ...) 
        -> Result<JobResultData, String>;
    async fn collect_result(&self, job_id: &str, timeout_secs: u64) 
        -> Result<CollectedResult, String>;
}

// 2. Adapter que implementa el port usando CommandBus
pub struct CommandBusJobExecutionPort {
    command_bus: DynCommandBus,
}

#[async_trait]
impl JobExecutionPort for CommandBusJobExecutionPort {
    // Implementación usando command_bus
}

// 3. Workflow solo conoce el port
pub struct ExecutionWorkflow {
    port: Arc<dyn JobExecutionPort>,
}
```

**Esuerzo**: 1 día  
**Prioridad**: MEDIA

---

## Violaciones de SRP

### DEBT-007: JobController con Múltiples Roles

**Archivo**: `crates/server/application/src/jobs/controller.rs`

**Estado**: **PARCIALMENTE REFATORIZADO** ✅

**Descripción**:
El código muestra que ya se ha refactorizado a un facade pattern. El comentario indica:
```rust
/// This is now a thin facade that delegates to specialized components:
/// - EventSubscriber: handles event subscription
/// - JobDispatcher: handles job dispatching
/// - WorkerMonitor: handles worker monitoring
/// - JobCoordinator: orchestrates the workflow
```

**Acción**: **COMPLETADO** - Mantener arquitectura actual

---

### DEBT-008: WorkerProviderConfig con Validación Mezclada

**Archivo**: `crates/server/domain/src/workers/provider_api.rs:103-238`

**Descripción**:
Los structs de config mezclan datos con validación

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Validación Separada =====

// 1. Configs son datos puros (POD)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfigExt {
    pub annotations: HashMap<String, String>,
    pub custom_labels: HashMap<String, String>,
    pub node_selector: HashMap<String, String>,
    // ... solo datos
}

// 2. Validador separado
pub struct KubernetesConfigValidator;

impl KubernetesConfigValidator {
    pub fn validate(config: &KubernetesConfigExt) -> Result<(), ValidationError> {
        // Reglas de validación
    }
}

// 3. Trait de validación
pub trait Validate: Send + Sync {
    fn validate(&self) -> Result<(), ValidationError>;
}

impl Validate for KubernetesConfigExt {
    fn validate(&self) -> Result<(), ValidationError> {
        KubernetesConfigValidator::validate(self)
    }
}
```

**Esuerzo**: 1 día  
**Prioridad**: BAJA

---

## Violaciones de OCP

### DEBT-009: ProviderFeature Enum

**Archivo**: `crates/server/domain/src/workers/provider_api.rs:364-437`

**Descripción**:
Agregar nuevos features requiere modificar el enum

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Type-Erased Feature Pattern =====

// 1. Trait para features
pub trait ProviderFeature: Send + Sync + Any {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
}

// 2. Features concretos implementan el trait
pub struct GpuFeature {
    pub vendor: GpuVendor,
    pub models: Vec<GpuModel>,
    pub max_count: u32,
}

impl ProviderFeature for GpuFeature {
    fn name(&self) -> &str { "gpu" }
    fn as_any(&self) -> &dyn Any { self }
}

// 3. ProviderCapabilities usa type erasure
pub struct ProviderCapabilities {
    pub max_resources: ResourceLimits,
    pub features: Vec<Box<dyn ProviderFeature>>, // Type-erased
}

// 4. Helper methods para recuperación type-safe
impl ProviderCapabilities {
    pub fn get_feature<T: ProviderFeature + 'static>(&self) -> Option<&T> {
        self.features
            .iter()
            .find_map(|f| f.as_any().downcast_ref::<T>())
    }
}
```

**Esuerzo**: 2 días  
**Prioridad**: MEDIA

---

### DEBT-010: SagaType Enum

**Archivo**: `crates/server/domain/src/saga/types.rs:79-104`

**Descripción**:
Agregar nuevos tipos de saga requiere modificar enum y match statements

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Registry Pattern =====

// 1. Trait para saga types
pub trait SagaType: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn is_provisioning(&self) -> bool { false }
    fn is_execution(&self) -> bool { false }
    // ... defaults
}

// 2. Saga types concretos
pub struct ProvisioningSagaType;
impl SagaType for ProvisioningSagaType {
    fn name(&self) -> &str { "PROVISIONING" }
    fn is_provisioning(&self) -> bool { true }
}

// 3. Registry para tipos dinámicos
pub struct SagaTypeRegistry {
    types: HashMap<String, Box<dyn SagaType>>,
}

impl SagaTypeRegistry {
    pub fn register<T: SagaType + Clone + 'static>(&mut self, saga_type: T) {
        let name = saga_type.name().to_string();
        self.types.insert(name, Box::new(saga_type));
    }
    
    pub fn get(&self, name: &str) -> Option<&dyn SagaType> {
        self.types.get(name).map(|b| b.as_ref())
    }
}
```

**Esuerzo**: 3 días  
**Prioridad**: BAJA (los enums actuales son manejables)

---

## Violaciones de LSP

### DEBT-011: State Mappers con Comportamiento Inconsistente

**Archivo**: `crates/server/domain/src/workers/provider_api.rs:563-690`

**Descripción**:
`DockerStateMapper` y `KubernetesStateMapper` manejan estados desconocidos diferente

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Strategy Pattern con Comportamiento Consistente =====

// 1. Trait con semántica clara
pub trait StateMapper<T>: Send + Sync {
    fn to_worker_state(&self, state: &T) -> WorkerState;
    
    // Comportamiento consistente para estados desconocidos
    fn map_unknown(&self) -> WorkerState {
        WorkerState::Creating // Default consistent
    }
}

// 2. Implementaciones con comportamiento explícito
pub struct DockerStateMapper;

impl StateMapper<&str> for DockerStateMapper {
    fn to_worker_state(&self, state: &&str) -> WorkerState {
        match *state {
            "running" => WorkerState::Ready,
            "exited" => WorkerState::Terminated,
            _ => self.map_unknown(), // Consistente
        }
    }
}

// 3. Tests para verificar sustituibilidad
#[cfg(test)]
mod lsp_tests {
    #[test]
    fn test_state_mappers_are_substitutable() {
        // Verificar que todos los mappers manejan unknown igual
        let docker = DockerStateMapper;
        let k8s = KubernetesStateMapper;
        
        assert_eq!(docker.map_unknown(), k8s.map_unknown());
    }
}
```

**Esuerzo**: 1 día  
**Prioridad**: BAJA

---

## Violaciones de DDD

### DEBT-012: Lógica de Dominio en Infraestructura

**Archivo**: `crates/server/infrastructure/src/providers/test_worker_provider.rs:195-238`

**Descripción**:
Lógica de procesos mezclada con infraestructura

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Delegación a Domain Services =====

// 1. Infrastructure solo maneja llamadas externas
pub struct TestWorkerProvider {
    client: HttpClient,
}

impl WorkerLifecycle for TestWorkerProvider {
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle> {
        // Solo llamada HTTP
        let response = self.client.post("/workers", spec).await?;
        Ok(WorkerHandle::from(response))
    }
}

// 2. Lógica de negocio en domain services
pub struct WorkerProvisioningService {
    provider: Arc<dyn WorkerLifecycle>,
    validator: Arc<dyn WorkerSpecValidator>,
}

impl WorkerProvisioningService {
    pub async fn provision_with_retry(&self, spec: &WorkerSpec) -> Result<WorkerHandle> {
        // Lógica de retry, validación, etc.
        self.validator.validate(spec)?;
        self.provider.create_worker(spec).await
    }
}
```

**Esuerzo**: 2 días  
**Prioridad**: MEDIA

---

### DEBT-013: Eventos de Dominio con Detalles de Implementación

**Archivo**: `crates/server/domain/src/workers/provider_api.rs:715-742`

**Descripción**:
`WorkerInfrastructureEvent` contiene metadata específica de provider

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Domain Events Puros =====

// 1. Eventos de dominio - Conceptos de negocio puros
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerDomainEvent {
    WorkerProvisioned {
        worker_id: WorkerId,
        provider_id: ProviderId,
        timestamp: DateTime<Utc>,
    },
    WorkerStarted {
        worker_id: WorkerId,
        timestamp: DateTime<Utc>,
    },
    WorkerTerminated {
        worker_id: WorkerId,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    WorkerUnhealthy {
        worker_id: WorkerId,
        reason: String,
        timestamp: DateTime<Utc>,
    },
}

// 2. Eventos de infraestructura - Separados
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerInfrastructureEvent {
    ProviderEvent {
        provider_id: ProviderId,
        event_type: String,
        payload: serde_json::Value, // Detalles específicos del provider
        timestamp: DateTime<Utc>,
    },
}

// 3. Adapter traduce de infraestructura a dominio
pub struct InfrastructureEventMapper;

impl InfrastructureEventMapper {
    pub fn to_domain_event(infra_event: WorkerInfrastructureEvent) 
        -> Option<WorkerDomainEvent> {
        // Traducción de detalles de infra a conceptos de dominio
    }
}
```

**Esuerzo**: 1 día  
**Prioridad**: MEDIA

---

### DEBT-014: Repository con Business Logic

**Archivo**: `crates/server/domain/src/workers/registry.rs:196-205`

**Descripción**:
`find_available` contiene reglas de negocio

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Query Object Pattern =====

// 1. Repository solo hace persistencia
#[async_trait]
pub trait WorkerRepository: Send + Sync {
    async fn find_all(&self) -> Result<Vec<Worker>>;
    async fn find_by_id(&self, id: &WorkerId) -> Result<Option<Worker>>;
    async fn find_by_state(&self, state: WorkerState) -> Result<Vec<Worker>>;
}

// 2. Query objects para lógica de negocio
pub struct FindAvailableQuery {
    repo: Arc<dyn WorkerRepository>,
}

impl FindAvailableQuery {
    pub async fn execute(&self, requirements: &JobRequirements) -> Result<Vec<Worker>> {
        let all_workers = self.repo.find_all().await?;
        
        // Lógica de filtrado (business logic)
        all_workers
            .into_iter()
            .filter(|w| self.is_eligible(w, requirements))
            .collect()
    }
    
    fn is_eligible(&self, worker: &Worker, requirements: &JobRequirements) -> bool {
        // Reglas de elegibilidad
    }
}
```

**Esuerzo**: 1 día  
**Prioridad**: MEDIA

---

## Problemas de Connascence

### DEBT-015: Connascence of Name - Inconsistencia de Nomenclatura

**Archivos**: Múltiples archivos en `saga/` y `workers/`

**Descripción**:
- `SagaId` vs `saga_id` (camelCase vs snake_case)
- `WorkerHandle` vs `ProviderWorkerInfo` (inconsistente)
- `JobResultData` vs `JobResultType` (confuso)

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Estándar de Nomenclatura =====

// 1. Definir estándares en README.md
// - Newtypes: PascalCase (ej: SagaId, WorkerId)
// - Structs: PascalCase (ej: WorkerHandle, JobResult)
// - Fields: snake_case (ej: saga_id, worker_handle)
// - Enums: PascalCase (ej: SagaType, WorkerState)

// 2. Aplicar consistentemente
pub struct SagaId(pub Uuid);        // ✓ Correcto
pub struct WorkerId(pub Uuid);      // ✓ Correcto

pub struct SagaContext {
    pub saga_id: SagaId,            // ✓ snake_case para campos
    pub worker_id: WorkerId,        // ✓ snake_case para campos
}

// 3. Herramienta de lint (clippy) para enforce
// #[warn(non_snake_case)] ya está habilitado
```

**Esuerzo**: 1 día (manual) + configuración de lints  
**Prioridad**: BAJA (pero mejora legibilidad)

---

### DEBT-016: Connascence of Type - Acoplamiento por serde_json::Value

**Archivo**: `crates/server/domain/src/saga/types.rs:42-71`

**Descripción**:
Múltiples tipos dependen de `serde_json::Value` para metadata

**Problema**:
```rust
pub struct SagaContext {
    pub metadata: HashMap<String, serde_json::Value>, // < Tipo genérico
}

// Cambios en serde afectan a todo
```

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Metadata Tipada =====

// 1. Metadata con tipos específicos por saga
pub trait SagaMetadata: Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

// 2. Metadata específica por tipo de saga
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningSagaMetadata {
    pub provider_id: ProviderId,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

impl SagaMetadata for ProvisioningSagaMetadata {
    fn as_any(&self) -> &dyn Any { self }
}

// 3. SagaContext usa type erasure
pub struct SagaContext {
    pub metadata: Option<Box<dyn SagaMetadata>>,
}

// 4. Helper para recuperar metadata tipada
impl SagaContext {
    pub fn get_metadata<T: SagaMetadata + 'static>(&self) -> Option<&T> {
        self.metadata
            .as_ref()
            .and_then(|m| m.as_any().downcast_ref::<T>())
    }
}
```

**Esuerzo**: 3 días  
**Prioridad**: MEDIA

---

### DEBT-017: Connascence of Position - Parámetros Orden-Dependientes

**Archivo**: `crates/server/application/src/saga/workflows/execution_durable.rs:159-200`

**Descripción**:
Los activity inputs dependen estrictamente del orden

**Problema**:
```rust
// El orden debe coincidir exactamente
let inputs = vec![
    job_id.to_string(),
    worker_id.to_string(),
    command.clone(),
    // ... 10+ parámetros
];

// Cambiar el orden rompe todo
```

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Parameter Object Pattern =====

// 1. Struct que encapsula parámetros relacionados
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDispatchParams {
    pub job_id: String,
    pub worker_id: String,
    pub command: String,
    pub arguments: Vec<String>,
    pub timeout_secs: u64,
}

// 2. Builders para construcción segura
impl JobDispatchParams {
    pub fn builder() -> JobDispatchParamsBuilder {
        JobDispatchParamsBuilder::default()
    }
}

// 3. Uso en activities
async fn dispatch_job_activity(params: JobDispatchParams) -> Result<JobResultData> {
    // Acceso por nombre, no por posición
    info!("Dispatching job {} to worker {}", params.job_id, params.worker_id);
}

// 4. Elimina CoP (Connascence of Position)
// Transforma a CoN (Connascence of Name) - más débil
```

**Esuerzo**: 2 días  
**Prioridad**: MEDIA

---

### DEBT-018: Connascence of Meaning - Enum Values Ambiguos

**Archivo**: `crates/server/domain/src/workers/provider_api.rs:365-437`

**Descripción**:
Algunos valores de `ProviderFeature` no son claros semánticamente

**Propuesta de Refactorización**:

```rust
// ===== SOLUCIÓN: Self-Documenting Code =====

// ANTES (ambiguo):
pub enum ProviderFeature {
    Custom(String), // ¿Qué significa Custom?
}

// DESPUÉS (claro):
pub enum ProviderFeature {
    Gpu { /* ... */ },
    CustomFpga {
        vendor: String,
        model: String,
        capabilities: Vec<String>,
    },
    CustomSgx {
        enclave_size: u64,
        family: String,
    },
}

// Self-documenting: El tipo documenta el significado
```

**Esuerzo**: 1 día  
**Prioridad**: BAJA

---

## Plan de Refactorización Prioritario

### Fase 1: Crítica (Semanas 1-2)
**Prioridad**: Resolver violaciones que bloquean EPIC-93

| ID | Tarea | Esfuerzo | Impacto |
|----|-------|----------|---------|
| DEBT-001 | WorkerProvider ISP segregation | 3-4 días | Alto |
| DEBT-004 | CommandBus abstraction | 1 día | Alto |
| DEBT-005 | PgPool → Repository pattern | 3 días | Alto |

**Total**: ~8 días

### Fase 2: Importante (Semanas 3-4)
**Prioridad**: Mejorar mantenibilidad

| ID | Tarea | Esfuerzo | Impacto |
|----|-------|----------|---------|
| DEBT-002 | WorkerProvisioningService segregation | 2 días | Medio |
| DEBT-003 | SagaContext decomposition | 2 días | Medio |
| DEBT-012 | Domain logic extraction | 2 días | Medio |
| DEBT-013 | Domain events purification | 1 día | Medio |
| DEBT-014 | Repository business logic removal | 1 día | Medio |

**Total**: ~8 días

### Fase 3: Mejora Continua (Semanas 5-6)
**Prioridad**: Reducir deuda técnica acumulada

| ID | Tarea | Esfuerzo | Impacto |
|----|-------|----------|---------|
| DEBT-006 | CommandBusJobExecutionPort adapter | 1 día | Medio |
| DEBT-009 | ProviderFeature type erasure | 2 días | Medio |
| DEBT-016 | Metadata tipada | 3 días | Medio |
| DEBT-017 | Parameter Object pattern | 2 días | Medio |

**Total**: ~8 días

### Fase 4: Limpieza (Semanas 7+)
**Prioridad**: Baja - puede hacerse incrementalmente

| ID | Tarea | Esfuerzo | Impacto |
|----|-------|----------|---------|
| DEBT-008 | Config validation separation | 1 día | Bajo |
| DEBT-010 | SagaType registry | 3 días | Bajo |
| DEBT-011 | State mapper consistency | 1 día | Bajo |
| DEBT-015 | Nomenclature standardization | 1 día | Bajo |
| DEBT-018 | Self-documenting enums | 1 día | Bajo |

**Total**: ~7 días

---

## Métricas de Deuda Técnica

### Deuda Actual
| Categoría | Ítems | Tiempo Estimado |
|-----------|-------|-----------------|
| ISP | 3 | 7-8 días |
| DIP | 3 | 5 días |
| SRP | 2 | 1 día (1 resuelto) |
| OCP | 2 | 5 días |
| LSP | 1 | 1 día |
| DDD | 3 | 4 días |
| Connascence | 4 | 7 días |
| **TOTAL** | **18** | **~31 días** |

### Deuda por Severidad
| Severidad | Ítems | % |
|-----------|-------|---|
| Alta | 3 | 17% |
| Media | 11 | 61% |
| Baja | 4 | 22% |

---

## Recomendaciones Estratégicas

### 1. Gobernanza de Código
Establecer **architecture decision records (ADRs)** para:
- Definición de nuevos traits (ISP compliance)
- Adición de métodos a interfaces existentes
- Patrones de inyección de dependencias
- Estándares de nomenclatura

### 2. Process de Review
Agregar checklist en PR reviews:
- [ ] ¿Este cambio cumple ISP?
- [ ] ¿El dominio no depende de infraestructura?
- [ ] ¿Se siguió DIP?
- [ ] ¿Se minimizó connascence?

### 3. Herramientas
- **clippy**: Habilitar más lints para SOLID
- **cargo-doc**: Documentar todos los traits públicos
- **rust-analyzer**: Configurar para detectar violaciones

### 4. Testing
- Cada refactor debe mantener **100% de coverage**
- Tests de integración para verificar composición
- Property-based tests para verificar LSP

---

## Conclusión

La deuda técnica identificada es **significativa pero manejable**. Con un plan estructurado de 6-8 semanas, es posible:

1. **Eliminar violaciones críticas** que bloquean EPIC-93
2. **Mejorar mantenibilidad** del código base
3. **Establecer patrones** para prevenir futura deuda
4. **Reducir connascence** fuerte a formas más débiles

`★ Insight ─────────────────────────────────────`
- **Inversión inteligente**: El tiempo invertido en refactor ahora pagará dividendos durante el desarrollo de EPIC-93 y features futuras
- **Conocimiento compartido**: Cada refactor es una oportunidad de aprendizaje para el equipo sobre SOLID y DDD
- **Deuda técnica = intereses compuestos**: No abordarla causa crecimiento exponencial del costo de cambio
`─────────────────────────────────────────────────`

---

**Documento mantenido por**: Arquitectura de Software  
**Última actualización**: 2026-01-22  
**Próxima revisión**: 2026-02-22 (mensual)
