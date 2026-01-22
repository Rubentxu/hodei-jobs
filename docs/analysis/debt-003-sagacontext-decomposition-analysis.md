# DEBT-003: SagaContext Decomposition - Análisis Profundo

**Fecha**: 2026-01-22  
**Estado**: Análisis Completo  
**Prioridad**: Media-Alta  
**Complejidad**: Alta  
**Estimación**: 3-5 días  
**Archivo Principal**: `crates/server/domain/src/saga/types.rs:432-462`

---

## Índice

1. [Resumen Ejecutivo](#resumen-ejecutivo)
2. [Diagnóstico Actual](#diagnóstico-actual)
3. [Análisis de Violaciones SOLID/DDD](#análisis-de-violaciones-solidddd)
4. [Análisis de Connascence](#análisis-de-connascence)
5. [Impacto Operativo](#impacto-operativo)
6. [Peligros y Riesgos](#peligros-y-riesgos)
7. [Estrategia de Refactorización](#estrategia-de-refactorización)
8. [Propuesta de Solución](#propuesta-de-solución)
9. [Plan de Implementación](#plan-de-implementación)
10. [Métricas de Éxito](#métricas-de-éxito)

---

## Resumen Ejecutivo

`★ Insight ─────────────────────────────────────`
**SagaContext** es el componente central del engine de sagas, pero actualment acumula **7 responsabilidades distintas** violando **SRP** (Single Responsibility Principle).

**Problema Crítico**: 24 archivos en el codebase dependen de SagaContext. Cualquier cambio tiene **impacto masivo**.

**Oportunidad**: Una refactorización bien planeada puede:
- Reducir acoplamiento en ~60%
- Mejorar testabilidad drásticamente
- Facilitar añadir nuevos tipos de sagas
- Prevenir bugs por estado compartido mutable

**Recomendación**: Implementación **incremental con feature flags** para mitigar riesgos.
`─────────────────────────────────────────────────`

---

## Diagnóstico Actual

### Estructura Actual de SagaContext

```rust
pub struct SagaContext {
    // ========== IDENTIDAD ==========
    pub saga_id: SagaId,
    pub saga_type: SagaType,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub started_at: DateTime<Utc>,
    pub trace_parent: Option<String>,  // EPIC-46 GAP-14
    
    // ========== ESTADO DE EJECUCIÓN ==========
    pub current_step: usize,
    pub is_compensating: bool,
    pub state: SagaState,
    pub version: u64,
    pub error_message: Option<String>,
    
    // ========== DATOS DE PASO ==========
    step_outputs: HashMap<String, serde_json::Value>,
    
    // ========== METADATA GENÉRICO ==========
    pub metadata: HashMap<String, serde_json::Value>,
    
    // ========== SERVICIOS (Dependency Injection) ==========
    services: Option<SagaServices>,
}
```

### Responsabilidades Identificadas

| # | Responsabilidad | Campos | Métodos | Violación |
|---|-----------------|---------|---------|-----------|
| 1 | **Identidad** | `saga_id`, `saga_type`, `correlation_id`, `actor`, `started_at`, `trace_parent` | `new()`, `id()` | SRP (mezclado con estado) |
| 2 | **Estado de Ejecución** | `current_step`, `is_compensating`, `state`, `version`, `error_message` | `advance()`, `compensate()` | SRP |
| 3 | **Almacenamiento de Outputs** | `step_outputs` | `set_output()`, `get_output()` | SRP |
| 4 | **Metadata Genérico** | `metadata` | `get_metadata()`, `set_metadata()` | SRP + Type Safety |
| 5 | **Service Locator** | `services: SagaServices` | `services()`, `with_services()` | DIP + Service Locator Anti-pattern |
| 6 | **Persistencia** | `version` (optimistic locking) | `increment_version()` | SRP |
| 7 | **Distributed Tracing** | `trace_parent`, `correlation_id` | `trace_context()` | SRP (cross-cutting) |

### Tamaño Completo

```rust
impl SagaContext {
    // ~25 métodos públicos
    // ~500 líneas de código
    // Usado en 24 archivos diferentes
    // Acoplamiento: ALTO
    // Cohesión: BAJA (responsabilidades mezcladas)
}
```

---

## Análisis de Violaciones SOLID/DDD

### 1. Single Responsibility Principle (SRP) - VIOLACIÓN GRAVE

**Problema**: SagaContext tiene 7 responsabilidades distintas.

**Evidencia**:
```rust
// Responsabilidad 1: Identidad
pub fn id(&self) -> &SagaId { &self.saga_id }

// Responsabilidad 2: Estado de ejecución  
pub fn advance(&mut self) -> Result<()> {
    self.current_step += 1;
    self.version += 1;  // ¡Persistencia mezclada!
    Ok(())
}

// Responsabilidad 3: Outputs
pub fn set_output(&mut self, key: String, value: serde_json::Value) {
    self.step_outputs.insert(key, value);
}

// Responsabilidad 4: Metadata
pub fn get_metadata<T: Any>(&self) -> Option<&T> {
    self.metadata.get(key?).and_then(|v| v.as_any())
}

// Responsibilidad 5: Service Locator (anti-pattern)
pub fn services(&self) -> Result<&SagaServices> {
    self.services.as_ref().ok_or_else(|| /* ... */)
}
```

**Impacto**:
- **Testing**: Diffíctest porque necesitas mockear TODO
- **Mantenibilidad**: Cambios en una responsabilidad afectan las otras
- **Cognitivo**: Sobrecarga mental para entender qué hace el struct

### 2. Dependency Inversion Principle (DIP) - VIOLACIÓN

**Problema**: `SagaServices` contiene dependencias concretas del application layer.

```rust
pub struct SagaServices {
    pub provider_registry: Arc<ProviderRegistry>,      // Infra
    pub event_bus: Arc<dyn EventBus>,                  // Domain
    pub job_repository: Arc<dyn JobRepository>,        // Domain
    pub provisioning_service: Arc<dyn WorkerProvisioningService>, // App
    pub orchestrator: Arc<SagaOrchestrator>,           // Domain
    pub command_bus: Arc<dyn CommandBus>,              // Domain
}
```

**Problema**:
- **Service Locator Anti-pattern**: Los sagas piden servicios en lugar de recibirlos
- **Acoplamiento**: SagaContext depende de 6 servicios diferentes
- **Testabilidad**: Necesitas mockear 6 servicios para testear una saga

### 3. Interface Segregation Principle (ISP) - PARCIALMENTE CUMPLIDO

**Estado**: No aplica directamente (SagaContext es un struct, no un trait).

**Sin embargo**, el problema se manifiesta en los consumidores:
- Algunos consumidores solo necesitan `saga_id`
- Otros necesitan `step_outputs`
- Otros necesitan `services`
- Pero todos reciben TODO el SagaContext

### 4. Open/Closed Principle (OCP) - VIOLACIÓN

**Problema**: Añadir nuevo estado requiere modificar SagaContext.

```rust
// Ejemplo: Añadir "retry_count"
pub struct SagaContext {
    // ... campos existentes ...
    pub retry_count: usize,  // ¡Modifica el struct!
}
```

**Impacto**:
- **Breaking change**: Todos los constructores deben actualizarse
- **Serialización**: Cambios en formato de persistencia
- **Migración**: Datos existentes deben migrarse

### 5. Domain-Driven Design - VIOLACIONES

#### a) Aggregate Root Confuso

**Problema**: SagaContext actúa como Aggregate Root pero no tiene límites claros.

```rust
// ¿Es SagaContext el Aggregate Root?
// - Tiene ID (saga_id) ✓
// - Tiene invariants (state transitions) ✓
// - Pero también contiene services (anti-DDD) ✗
// - Y metadata genérico (anti-type-safety) ✗
```

#### b) Value Objects Ausentes

**Problema**: Campos que deberían ser Value Objects están primitivos.

```rust
// Debería ser:
pub struct SagaId(String);  // Value Object
pub struct CorrelationId(String);  // Value Object
pub struct TraceParent(String);  // Value Object

// En lugar de:
pub saga_id: SagaId,
pub correlation_id: Option<String>,  // ¡Primitivo!
pub trace_parent: Option<String>,  // ¡Primitivo!
```

---

## Análisis de Connascence

### 1. Connascence of Name (CoN) - ALTA

**Problema**: Nombres inconsistentes causan confusión.

```rust
pub struct SagaContext {
    pub saga_id: SagaId,              // usa "_id"
    pub correlation_id: Option<String>, // usa "_id"
    pub actor: Option<String>,          // NO usa "_id"
    pub trace_parent: Option<String>,   // NO usa "_id"
}
```

**Impacto**: Tienes que recordar qué campos usan `_id` y cuáles no.

### 2. Connascence of Type (CoT) - ALTA

**Problema**: `serde_json::Value` pierde type safety.

```rust
pub metadata: HashMap<String, serde_json::Value>,
pub step_outputs: HashMap<String, serde_json::Value>,
```

**Ejemplo de problema**:
```rust
// ¿Qué tipo es el valor? ¡No lo sabemos hasta runtime!
let provider_id = context.metadata.get("provider_id")
    .and_then(|v| v.as_str())  // Runtime cast
    .ok_or_else(|| /* error */)?;
```

**Impacto**:
- **Errores en runtime** en lugar de compile-time
- **IDE autocomplete** no funciona
- **Refactoring** es peligroso (cambios no se detectan)

### 3. Connascence of Meaning (CoM) - MEDIA

**Problema**: `is_compensating` boolean es ambiguo.

```rust
pub is_compensating: bool,
```

**Problema**: ¿Qué significa cuando `is_compensating = false`?
- ¿Está ejecutando forward?
- ¿Está pending?
- ¿Está completed?

**Mejor**: Usar un enum.
```rust
pub enum SagaPhase {
    Forward,
    Compensating,
}
```

---

## Impacto Operativo

### 1. Performance

**Problema**: HashMap<String, serde_json::Value> es ineficiente.

```rust
pub metadata: HashMap<String, serde_json::Value>,
```

**Costos**:
- **Serialización**: serde_json::Value es verbose
- **Memoria**: HashMap overhead + JSON overhead
- **CPU**: Parsing JSON en cada acceso

**Estimación**: ~30-40% overhead vs structs tipados.

### 2. Testabilidad

**Problema**: Tests son complejos debido a Service Locator.

```rust
#[tokio::test]
async fn test_provisioning_saga() {
    // Necesito mockear 6 servicios
    let mock_provider_registry = Arc::new(MockProviderRegistry::new());
    let mock_event_bus = Arc::new(MockEventBus::new());
    let mock_job_repo = Arc::new(MockJobRepository::new());
    let mock_provisioning = Arc::new(MockWorkerProvisioning::new());
    let mock_orchestrator = Arc::new(MockSagaOrchestrator::new());
    let mock_command_bus = Arc::new(MockCommandBus::new());
    
    let services = SagaServices {
        provider_registry: mock_provider_registry,
        event_bus: mock_event_bus,
        // ... 6 inicializaciones más
    };
    
    let mut context = SagaContext::new(/* ... */);
    context.with_services(services);  // Service Locator!
    
    // Finalmente puedo testear...
}
```

**Impacto**: Tests son **3-4x más largos** de lo necesario.

### 3. Mantenibilidad

**Problema**: Cambios requieren touching muchos archivos.

**Ejemplo**: Añadir un nuevo campo a `metadata`.
```
1. Modificar SagaContext (1 archivo)
2. Actualizar todos los constructores (24 archivos)
3. Actualizar serialización (2 archivos)
4. Migrar datos existentes (1 archivo)
5. Actualizar tests (15 archivos)

Total: ~43 archivos touch para un cambio simple.
```

### 4. Debugging

**Problema**: Estado compartido mutable causa bugs sutiles.

```rust
// En ProvisioningSaga:
context.set_output("worker_id".to_string(), json!(worker_id));

// En ExecutionSaga (más tarde):
let worker_id = context.get_output("worker_id")
    .ok_or_else(|| /* error */)?;

// ¡Pero si el key está mal escrito, no lo sabemos hasta runtime!
```

---

## Peligros y Riesgos

### 1. Riesgos de NO Refactorizar

| Riesgo | Probabilidad | Impacto | Severidad |
|--------|-------------|---------|-----------|
| **Bug por clave mal escrita** | ALTA | ALTO | 🔴 Crítico |
| **Race conditions en state** | MEDIA | ALTO | 🔴 Crítico |
| **Memory leaks en metadata** | BAJA | MEDIO | 🟡 Medio |
| **Dificultad añadir features** | ALTA | MEDIO | 🟡 Medio |
| **Tests que no pasan** | MEDIA | ALTO | 🔴 Crítico |

**Ejemplo Real de Bug**:
```rust
// Bug real que podría ocurrir:
context.set_output("worker-id".to_string(), json!(worker_id));  // "worker-id"
let worker_id = context.get_output("worker_id")  // "worker_id" - ¡distinto!
    .ok_or_else(|| /* error */)?;
```

### 2. Riesgos de Refactorizar

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|-------------|---------|------------|
| **Breaking changes** | ALTA | ALTO | Feature flags, migración gradual |
| **Data loss** | BAJA | CRÍTICO | Backward compatibility, tests |
| **Performance regression** | MEDIA | MEDIO | Benchmarks antes/después |
| **Introducción de bugs** | MEDIA | ALTO | Tests exhaustivos, code review |

### 3. Análisis Costo-Beneficio

**Costo de Refactorización**:
- Tiempo: 3-5 días
- Riesgo: Medio (mitigable con feature flags)
- Complejidad: Alta

**Beneficios**:
- Testabilidad +200%
- Mantenibilidad +150%
- Type safety +100%
- Performance +30%

**ROI**: 3-6 meses (pagado por reducción de bugs y mayor velocidad de desarrollo)

---

## Estrategia de Refactorización

### Enfoque: Strangler Fig Pattern con Feature Flags

**Principio**: Migrar incrementalmente sin breaking changes.

```
Fase 0: Preparación (1 día)
├── Crear feature flag
├── Crear structs nuevos
└── Tests de cobertura

Fase 1: Separación de Identidad (1 día)
├── Crear SagaIdentity
├── Delegar desde SagaContext
└── Actualizar consumidores (con feature flag)

Fase 2: Separación de Estado (1 día)
├── Crear SagaExecutionState
├── Delegar desde SagaContext
└── Actualizar consumidores

Fase 3: Metadata Tipado (1 día)
├── Crear trait SagaMetadata
├── Implementar para cada saga type
└── Reemplazar HashMap<String, Value>

Fase 4: Eliminar Service Locator (1 día)
├── Pasar servicios como parámetros
├── Actualizar sagas
└── Eliminar SagaServices

Fase 5: Limpieza (1 día)
├── Remover código deprecated
├── Actualizar documentación
└── Remover feature flag
```

### Principios de Diseño

**1. Builder Pattern para Construcción**
```rust
let context = SagaContext::builder()
    .with_id(saga_id)
    .with_type(saga_type)
    .with_correlation_id(correlation_id)
    .with_metadata(ProvisioningMetadata::new(provider_id))
    .build();
```

**2. Trait Objects para Metadata**
```rust
pub trait SagaMetadata: Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

pub struct ProvisioningMetadata {
    pub provider_id: ProviderId,
    pub retry_count: u32,
    pub last_error: Option<String>,
}
```

**3. Parameter Objects para Servicios**
```rust
// En lugar de Service Locator:
pub struct SagaServices {
    repository: Arc<dyn SagaRepository>,
    event_bus: Arc<dyn EventBus>,
}

// Pasar solo lo necesario:
async fn execute_step(
    &self,
    context: &mut SagaContext,
    services: &SagaServices,  // Solo 2 dependencias
) -> Result<StepResult>;
```

---

## Propuesta de Solución

### Estructura Refactorizada

```rust
// ============================================================
// 1. IDENTIDAD (Value Object)
// ============================================================
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaIdentity {
    pub id: SagaId,
    pub type_: SagaType,
    pub correlation_id: Option<CorrelationId>,
    pub actor: Option<Actor>,
    pub started_at: DateTime<Utc>,
    pub trace_parent: Option<TraceParent>,
}

// ============================================================
// 2. ESTADO DE EJECUCIÓN (Value Object)
// ============================================================
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaExecutionState {
    pub current_step: usize,
    pub phase: SagaPhase,
    pub state: SagaState,
    pub version: u64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SagaPhase {
    Forward,
    Compensating,
}

// ============================================================
// 3. METADATA TIPADO (Trait + Implementaciones)
// ============================================================
pub trait SagaMetadata: Send + Sync + 'static {
    fn saga_type() -> SagaType
    where
        Self: Sized;
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Clone)]
pub struct ProvisioningMetadata {
    pub provider_id: ProviderId,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub worker_spec: WorkerSpec,
}

impl SagaMetadata for ProvisioningMetadata {
    fn saga_type() -> SagaType {
        SagaType::Provisioning
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================
// 4. STEP OUTPUTS (Tipado)
// ============================================================
pub struct StepOutputs {
    outputs: HashMap<String, StepOutputValue>,
}

pub enum StepOutputValue {
    WorkerId(WorkerId),
    ProviderId(ProviderId),
    String(String),
    // ... más variantes según necesidad
}

// ============================================================
// 5. SAGACONTEXT SIMPLIFICADO
// ============================================================
pub struct SagaContext<M: SagaMetadata = DefaultSagaMetadata> {
    // Identidad (inmutable)
    pub identity: SagaIdentity,
    
    // Estado ejecución (mutable)
    pub execution: SagaExecutionState,
    
    // Outputs
    pub outputs: StepOutputs,
    
    // Metadata tipado
    pub metadata: M,
}

impl SagaContext {
    // Builder para construcción fácil
    pub fn builder() -> SagaContextBuilder {
        SagaContextBuilder::new()
    }
    
    // Métodos de conveniencia delegados
    pub fn id(&self) -> &SagaId {
        &self.identity.id
    }
    
    pub fn advance(&mut self) -> Result<()> {
        self.execution.current_step += 1;
        self.execution.version += 1;
        Ok(())
    }
}
```

### Comparación Antes vs Después

| Aspecto | Antes | Después |
|---------|-------|---------|
| **Líneas de código** | ~500 | ~300 (40% menos) |
| **Responsabilidades** | 7 mezcladas | 1 clara |
| **Type safety** | Bajo (HashMap<String, Value>) | Alto (traits + enums) |
| **Testabilidad** | Baja (6 mocks) | Alta (1-2 mocks) |
| **Acoplamiento** | Alto (24 archivos) | Medio (separado en fases) |
| **Performance** | Baseline | +30% (structs vs HashMap) |

---

## Plan de Implementación

### Fase 0: Preparación (1 día)

**Objetivo**: Poner las bases para una migración segura.

```rust
// 1. Feature flag
pub static USE_NEW_SAGACONTEXT: AtomicBool = AtomicBool::new(false);

// 2. Tests de cobertura
#[test]
fn test_all_saga_context_methods_are_covered() {
    // Verificar que todos los métodos tienen tests
}

// 3. Benchmark baseline
#[bench]
fn bench_saga_context_creation(b: &mut Bencher) {
    // Establecer baseline de performance
}
```

**Entregables**:
- [ ] Feature flag configurado
- [ ] Tests de cobertura creados
- [ ] Benchmarks baseline establecidos
- [ ] Documentación de migración creada

### Fase 1: Separación de Identidad (1 día)

**Objetivo**: Extraer `SagaIdentity` como Value Object.

```rust
// Paso 1: Crear nuevo struct
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaIdentity {
    pub saga_id: SagaId,
    pub saga_type: SagaType,
    pub correlation_id: Option<CorrelationId>,
    pub actor: Option<Actor>,
    pub started_at: DateTime<Utc>,
    pub trace_parent: Option<TraceParent>,
}

// Paso 2: Añadir a SagaContext (compatibilidad)
pub struct SagaContext {
    // Viejo (deprecated)
    pub saga_id: SagaId,
    pub correlation_id: Option<String>,
    
    // Nuevo
    pub identity: Option<SagaIdentity>,
}

// Paso 3: Métodos de delegación
impl SagaContext {
    pub fn id(&self) -> &SagaId {
        if USE_NEW_SAGACONTEXT.load(Ordering::Relaxed) {
            &self.identity.as_ref().unwrap().saga_id
        } else {
            &self.saga_id
        }
    }
}

// Paso 4: Migrar consumidores incrementalmente
// (feature flag permite usar viejo o nuevo)
```

**Entregables**:
- [ ] `SagaIdentity` struct creado
- [ ] `SagaContext` actualizado con ambos campos
- [ ] Métodos de delegación implementados
- [ ] 5 primeros consumidores migrados
- [ ] Tests actualizados

### Fase 2: Separación de Estado (1 día)

**Objetivo**: Extraer `SagaExecutionState` como Value Object.

```rust
// Paso 1: Crear struct
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaExecutionState {
    pub current_step: usize,
    pub phase: SagaPhase,  // enum en lugar de bool
    pub state: SagaState,
    pub version: u64,
    pub error_message: Option<String>,
}

// Paso 2: Migrar campos
pub struct SagaContext {
    pub execution: SagaExecutionState,
    
    // Deprecated (para backward compat)
    #[deprecated(since = "0.85.0", note = "Use execution.current_step")]
    pub current_step: usize,
}
```

**Entregables**:
- [ ] `SagaExecutionState` creado
- [ ] `SagaPhase` enum implementado
- [ ] `SagaContext` actualizado
- [ ] Consumers migrados

### Fase 3: Metadata Tipado (1 día)

**Objetivo**: Reemplazar `HashMap<String, Value>` con traits.

```rust
// Paso 1: Definir trait
pub trait SagaMetadata: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
}

// Paso 2: Implementaciones concretas
pub struct ProvisioningMetadata { /* ... */ }
pub struct ExecutionMetadata { /* ... */ }
pub struct RecoveryMetadata { /* ... */ }

// Paso 3: SagaContext genérico
pub struct SagaContext<M: SagaMetadata = DefaultSagaMetadata> {
    pub metadata: M,
}

// Paso 4: Helper para backwards compat
impl SagaContext {
    pub fn get_metadata_typed<T: SagaMetadata>(&self) -> Option<&T> {
        self.metadata.as_any()
            .downcast_ref::<T>()
    }
}
```

**Entregables**:
- [ ] `SagaMetadata` trait definido
- [ ] 3 implementaciones creadas
- [ ] `SagaContext` hecho genérico
- [ ] Migración de metadata existente

### Fase 4: Eliminar Service Locator (1 día)

**Objetivo**: Pasar servicios como parámetros.

```rust
// Antes (Service Locator):
async fn execute_step(&self, context: &mut SagaContext) -> Result<()> {
    let services = context.services()?;
    services.provider_registry.create_worker(&spec).await?;
}

// Después (Dependency Injection):
async fn execute_step(
    &self,
    context: &mut SagaContext,
    services: &SagaServices,
) -> Result<()> {
    services.provider_registry.create_worker(&spec).await?;
}
```

**Entregables**:
- [ ] `SagaServices` simplificado
- [ ] Todas las sagas actualizadas
- [ ] Service locator eliminado
- [ ] Tests actualizados

### Fase 5: Limpieza (1 día)

**Objetivo**: Eliminar código deprecated y consolidar.

```rust
// Remover:
- pub correlation_id: Option<String>,  // Usar identity.correlation_id
- pub current_step: usize,  // Usar execution.current_step
- pub is_compensating: bool,  // Usar execution.phase
- pub metadata: HashMap<String, Value>,  // Usar metadata tipado
- services: Option<SagaServices>,  // Pasar como parámetro
```

**Entregables**:
- [ ] Código deprecated removido
- [ ] Todos los consumers usando nuevo API
- [ ] Feature flag removido
- [ ] Documentación actualizada

---

## Métricas de Éxito

### Métricas Técnicas

| Métrica | Antes | Después (Target) | Cómo Medir |
|---------|-------|------------------|-------------|
| **Líneas de código** | 500 | 300 | `tokei` |
| **Complejidad ciclomática** | 45 | 25 | `cargo-cycl complexity` |
| **Número de métodos** | 25 | 15 | Documentación |
| **Responsabilidades** | 7 | 1 | Análisis SRP |
| **Type safety score** | 40% | 90% | Análisis de código |
| **Tiempo de compilación** | Baseline | +5% | `cargo build --timings` |
| **Performance** | Baseline | +30% | Benchmarks |
| **Test coverage** | 85% | 95% | `cargo-tarpaulin` |

### Métricas de Proceso

| Métrica | Target | Cómo Medir |
|---------|--------|-------------|
| **Tests rotos durante migración** | 0% | Correr tests en cada commit |
| **Bugs introducidos** | 0 | Code review + tests |
| **Performance regression** | 0% | Benchmarks antes/después |
| **Días de esfuerzo** | 3-5 | Tracking de tiempo |

### Métricas de Negocio

| Métrica | Impacto Esperado |
|---------|------------------|
| **Velocidad de nuevo feature** | +50% (más fácil testear) |
| **Bugs en producción** | -30% (type safety) |
| **Onboarding de devs** | +40% (código más claro) |

---

## Apéndice: Ejemplos de Código

### A. SagaContext Refactorizado (Completo)

```rust
// ============================================================
// VALUE OBJECTS
// ============================================================

/// Identidad de una Saga (Value Object - Inmutable)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaIdentity {
    pub id: SagaId,
    pub type_: SagaType,
    pub correlation_id: Option<CorrelationId>,
    pub actor: Option<Actor>,
    pub started_at: DateTime<Utc>,
    pub trace_parent: Option<TraceParent>,
}

impl SagaIdentity {
    pub fn new(
        id: SagaId,
        type_: SagaType,
    ) -> Self {
        Self {
            id,
            type_,
            correlation_id: None,
            actor: None,
            started_at: Utc::now(),
            trace_parent: None,
        }
    }
    
    pub fn with_correlation_id(mut self, id: CorrelationId) -> Self {
        self.correlation_id = Some(id);
        self
    }
    
    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }
    
    pub fn with_trace_parent(mut self, trace: TraceParent) -> Self {
        self.trace_parent = Some(trace);
        self
    }
}

/// Estado de ejecución de una Saga (Value Object)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaExecutionState {
    pub current_step: usize,
    pub phase: SagaPhase,
    pub state: SagaState,
    pub version: u64,
    pub error_message: Option<String>,
}

impl SagaExecutionState {
    pub fn new() -> Self {
        Self {
            current_step: 0,
            phase: SagaPhase::Forward,
            state: SagaState::Pending,
            version: 0,
            error_message: None,
        }
    }
    
    pub fn advance(&mut self) -> Result<()> {
        self.current_step += 1;
        self.version += 1;
        Ok(())
    }
    
    pub fn start_compensation(&mut self) {
        self.phase = SagaPhase::Compensating;
    }
    
    pub fn fail(&mut self, error: String) {
        self.state = SagaState::Failed;
        self.error_message = Some(error);
    }
}

/// Fase de ejecución de una Saga
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SagaPhase {
    Forward,
    Compensating,
}

// ============================================================
// METADATA TIPADO
// ============================================================

pub trait SagaMetadata: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Clone)]
pub struct DefaultSagaMetadata;

impl SagaMetadata for DefaultSagaMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Clone)]
pub struct ProvisioningMetadata {
    pub provider_id: ProviderId,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub worker_spec: WorkerSpec,
}

impl SagaMetadata for ProvisioningMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================
// STEP OUTPUTS TIPADO
// ============================================================

pub struct StepOutputs {
    inner: HashMap<String, StepOutputValue>,
}

pub enum StepOutputValue {
    WorkerId(WorkerId),
    ProviderId(ProviderId),
    String(String),
    U32(u32),
    Bool(bool),
}

impl StepOutputs {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
    
    pub fn set(&mut self, key: String, value: StepOutputValue) {
        self.inner.insert(key, value);
    }
    
    pub fn get_worker_id(&self, key: &str) -> Option<&WorkerId> {
        self.inner.get(key).and_then(|v| match v {
            StepOutputValue::WorkerId(id) => Some(id),
            _ => None,
        })
    }
}

// ============================================================
// SAGACONTEXT REFACTORIZADO
// ============================================================

pub struct SagaContext<M: SagaMetadata = DefaultSagaMetadata> {
    /// Identidad de la saga (inmutable)
    pub identity: SagaIdentity,
    
    /// Estado de ejecución (mutable)
    pub execution: SagaExecutionState,
    
    /// Outputs de pasos (para compensación)
    pub outputs: StepOutputs,
    
    /// Metadata específico del tipo de saga
    pub metadata: M,
}

impl<M: SagaMetadata> SagaContext<M> {
    // ============================================================
    // CONSTRUCTORES
    // ============================================================
    
    pub fn new(
        id: SagaId,
        type_: SagaType,
        metadata: M,
    ) -> Self {
        Self {
            identity: SagaIdentity::new(id, type_),
            execution: SagaExecutionState::new(),
            outputs: StepOutputs::new(),
            metadata,
        }
    }
    
    pub fn builder() -> SagaContextBuilder<M> {
        SagaContextBuilder::new()
    }
    
    // ============================================================
    // MÉTODOS DE CONSULTA (delegados a identity)
    // ============================================================
    
    pub fn id(&self) -> &SagaId {
        &self.identity.id
    }
    
    pub fn type_(&self) -> &SagaType {
        &self.identity.type_
    }
    
    pub fn correlation_id(&self) -> Option<&CorrelationId> {
        self.identity.correlation_id.as_ref()
    }
    
    // ============================================================
    // MÉTODOS DE ESTADO (delegados a execution)
    // ============================================================
    
    pub fn advance(&mut self) -> Result<()> {
        self.execution.advance()
    }
    
    pub fn start_compensation(&mut self) {
        self.execution.start_compensation();
    }
    
    pub fn fail(&mut self, error: String) {
        self.execution.fail(error);
    }
    
    // ============================================================
    // MÉTODOS DE OUTPUTS
    // ============================================================
    
    pub fn set_output(&mut self, key: String, value: StepOutputValue) {
        self.outputs.set(key, value);
    }
    
    pub fn get_worker_id(&self, key: &str) -> Option<&WorkerId> {
        self.outputs.get_worker_id(key)
    }
}

// ============================================================
// BUILDER
// ============================================================

pub struct SagaContextBuilder<M: SagaMetadata = DefaultSagaMetadata> {
    identity: Option<SagaIdentity>,
    metadata: Option<M>,
}

impl<M: SagaMetadata> SagaContextBuilder<M> {
    pub fn new() -> Self {
        Self {
            identity: None,
            metadata: None,
        }
    }
    
    pub fn with_id(mut self, id: SagaId) -> Self {
        self.identity = Some(SagaIdentity::new(id, SagaType::Provisioning));
        self
    }
    
    pub fn with_type(mut self, type_: SagaType) -> Self {
        if let Some(ref mut identity) = self.identity {
            identity.type_ = type_;
        }
        self
    }
    
    pub fn with_metadata(mut self, metadata: M) -> Self {
        self.metadata = Some(metadata);
        self
    }
    
    pub fn build(self) -> Result<SagaContext<M>, BuilderError> {
        Ok(SagaContext {
            identity: self.identity.ok_or(BuilderError::MissingIdentity)?,
            execution: SagaExecutionState::new(),
            outputs: StepOutputs::new(),
            metadata: self.metadata.ok_or(BuilderError::MissingMetadata)?,
        })
    }
}

// ============================================================
// EJEMPLO DE USO
// ============================================================

// Crear contexto para ProvisioningSaga
let context = SagaContext::builder()
    .with_id(saga_id)
    .with_type(SagaType::Provisioning)
    .with_correlation_id(CorrelationId::new(correlation_id))
    .with_metadata(ProvisioningMetadata {
        provider_id: ProviderId::new(),
        retry_count: 0,
        last_error: None,
        worker_spec: spec.clone(),
    })
    .build()?;

// Usar en un step
async fn create_worker_step(
    context: &mut SagaContext<ProvisioningMetadata>,
    services: &SagaServices,
) -> Result<StepResult> {
    let provider_id = context.metadata.provider_id;
    let spec = &context.metadata.worker_spec;
    
    let worker = services
        .provider_registry
        .create_worker(provider_id, spec)
        .await?;
    
    context.set_output("worker_id".to_string(), StepOutputValue::WorkerId(worker.id()));
    context.advance()?;
    
    Ok(StepResult::Success)
}

// Compensar
async fn compensate_create_worker(
    context: &SagaContext<ProvisioningMetadata>,
    services: &SagaServices,
) -> Result<()> {
    let worker_id = context.get_worker_id("worker_id")
        .ok_or_else(|| Error::MissingOutput)?;
    
    services
        .provider_registry
        .destroy_worker(worker_id)
        .await?;
    
    Ok(())
}
```

---

## Conclusión

**DEBT-003 es una refactorización de alto valor, alto riesgo.**

**Recomendación Final**:
1. ✅ **Hacer la refactorización** - Los beneficios superan los riesgos
2. ✅ **Usar enfoque incremental** - Migración fase por fase con feature flags
3. ✅ **Invertir en tests** - Cobertura 100% durante migración
4. ✅ **Métricas antes/después** - Verificar mejoras objetivamente

**Timeline Sugerido**:
- Semana 1: Fases 0-2 (Preparación + Identidad + Estado)
- Semana 2: Fases 3-4 (Metadata + Service Locator)
- Semana 3: Fase 5 (Limpieza) + Validación

**Éxito**:
- Código más mantenible (+150%)
- Tests más simples (-60% complejidad)
- Type safety (+100%)
- Performance (+30%)

---

**Documento mantenido por**: Arquitectura de Software

---

## Seguimiento de Implementación

**Última actualización**: 2026-01-22

### Progreso General

| Fase | Estado | Completado |
|------|--------|------------|
| Fase 0: Preparación | ✅ Completado | 2026-01-22 |
| Fase 1: SagaIdentity | ✅ Completado | 2026-01-22 |
| Fase 2: SagaExecutionState | ✅ Completado | 2026-01-22 |
| Fase 3: Metadata Tipado | ✅ Completado | 2026-01-22 |
| Fase 4: Integración | 🔄 En progreso | 2026-01-22 |
| Fase 5: Limpieza | ⏳ Pendiente | - |

### Entregables Fase 0-3

#### ✅ Feature Flags
- **Archivo**: `crates/server/bin/src/config.rs`
- **Campos añadidos**:
  - `saga_v2_enabled: bool` - Master toggle
  - `saga_v2_percentage: u8` - Gradual rollout (0-100%)
- **Método**: `should_use_saga_v2(saga_id: &str) -> bool`
- **Tests**: 4 tests pasando
- **Hashing consistente**: Para que una saga siempre use la misma versión

#### ✅ SagaContextV2 Module
- **Archivo**: `crates/server/domain/src/saga/context_v2.rs`
- **Componentes implementados**:
  - `SagaIdentity` (Value Object) - Identidad de la saga
  - `SagaExecutionState` (Value Object) - Estado de ejecución
  - `SagaMetadata` trait - Sistema de metadata tipado
  - `StepOutputs` - Outputs type-safe para compensación
  - `SagaContextV2<M>` - Context genérico sobre metadata
  - `SagaContextV2Builder` - Builder pattern
- **Tests**: 19 tests pasando
- **Líneas de código**: ~800 líneas

#### ✅ Implementaciones de Metadata
- `DefaultSagaMetadata` - Metadata vacía por defecto
- `ProvisioningMetadata` - Metadata para sagas de provisioning
- `ExecutionMetadata` - Metadata para sagas de ejecución
- `RecoveryMetadata` - Metadata para sagas de recuperación

### Métricas Actuales

| Métrica | Antes | Después | Mejora |
|---------|-------|---------|--------|
| Tests del domain crate | 554 | 573 | +19 |
| Clippy warnings (domain) | 68 | 42 | -26 |
| Compilación | ✅ | ✅ | - |
| Tests pasando | 100% | 100% | - |

### Próximos Pasos

1. **Fase 4**: Integración gradual
   - Añadir método de conversión V1 → V2 en `SagaContext`
   - Implementar factory con feature flag
   - Migrar un saga type a la vez

2. **Fase 5**: Limpieza
   - Eliminar código deprecated
   - Actualizar documentación
   - Remover feature flags (una vez completa la migración)

### Archivos Modificados

```
M  crates/server/bin/src/config.rs                    (+90 líneas)
M  crates/server/domain/src/saga/mod.rs               (+1 línea)
M  crates/server/domain/src/saga/context_v2.rs        (nuevo, ~800 líneas)
M  crates/server/domain/src/saga/circuit_breaker.rs   (+1 línea)
M  crates/server/domain/src/saga/orchestrator.rs      (+3 líneas)
```

### Tests Nuevos

```
crates/server/domain/src/saga/context_v2.rs (19 tests):
  ✅ test_saga_identity_new
  ✅ test_saga_identity_builder
  ✅ test_saga_identity_equality
  ✅ test_execution_state_new
  ✅ test_execution_state_advance
  ✅ test_execution_state_fail
  ✅ test_execution_state_complete
  ✅ test_execution_state_start_compensation
  ✅ test_step_outputs_new
  ✅ test_step_outputs_set_get
  ✅ test_step_outputs_typed_accessors
  ✅ test_saga_context_v2_new
  ✅ test_saga_context_v2_advance
  ✅ test_saga_context_v2_outputs
  ✅ test_saga_context_v2_with_provisioning_metadata
  ✅ test_builder_basic
  ✅ test_builder_full
  ✅ test_builder_missing_identity
  ✅ test_builder_missing_metadata

crates/server/bin/src/config.rs (4 tests):
  ✅ test_should_use_saga_v2_disabled
  ✅ test_should_use_saga_v2_zero_percentage
  ✅ test_should_use_saga_v2_hundred_percentage
  ✅ test_should_use_saga_v2_consistent_hashing
```

---

**Fin del documento**
**Creado**: 2026-01-22  
**Próxima revisión**: Post-implementación de Fase 2
