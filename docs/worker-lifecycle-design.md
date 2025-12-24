# Worker Lifecycle Management Design

## Resumen Ejecutivo

Diseño de un sistema abstracto de **lifecycle management** y **garbage collection** para workers efímeros, siguiendo principios SOLID y DDD. Este sistema asegura que todos los workers sean verdaderamente efímeros con limpieza automática, ordenada y eficiente, incluso en casos de falla.

## Arquitectura

### 1. Dominio (Domain Layer)

#### Interfaces Abstratas (DDD)

```rust
// Lifecycle States
enum LifecycleState {
    Created, Initializing, Ready, Running,
    Terminating, Terminated, Failed
}

// Event Sourcing para lifecycle
enum LifecycleEvent {
    WorkerCreated, WorkerReady, WorkerTerminating,
    WorkerTerminated, WorkerCleanedUp, OrphanWorkerDetected
}

// Gestión de políticas efímeras
struct EphemeralWorkerPolicy {
    max_lifetime: Duration,
    ttl_after_completion: Option<Duration>,
    idle_timeout: Duration,
    auto_cleanup_enabled: bool,
    orphan_detection_enabled: bool
}
```

#### Principios DDD Aplicados

- **Event Sourcing**: Todos los cambios de estado son eventos
- **Aggregate Pattern**: Worker es un aggregate root
- **Domain Events**: Eventos de lifecycle publicados como domain events
- **Value Objects**: Policy, State como value objects inmutables

### 2. Capa de Aplicación (Application Layer)

#### Lifecycle Adapter (Decorator Pattern)

```rust
struct LifecycleManagedWorkerProvider<T> {
    inner: T,  // WorkerProvider existente
    lifecycle_manager: Arc<dyn EphemeralWorkerManager>,
    event_publisher: Arc<dyn LifecycleEventPublisher>
}
```

**Beneficios**:
- ✅ No modifica providers existentes (Open/Closed Principle)
- ✅ Decorator pattern para añadir funcionalidad (Single Responsibility)
- ✅ Interface Segregation - interfaces pequeñas y enfocadas

#### Interfaces Segregadas (SOLID)

```rust
trait WorkerLifecycleManager {
    async fn initialize_worker(...);
    async fn mark_worker_ready(...);
    async fn start_termination(...);
}

trait GarbageCollector {
    async fn run_gc_cycle(...);
    async fn cleanup_worker(...);
    async fn cleanup_orphans(...);
}

trait OrphanDetector {
    async fn detect_orphans(...) -> Vec<OrphanInfo>;
}
```

**Principios SOLID**:
- **S**: Single Responsibility - cada interface tiene una responsabilidad
- **O**: Open/Closed - extensible sin modificar
- **L**: Liskov Substitution - cualquier implementación es intercambiable
- **I**: Interface Segregation - interfaces pequeñas y específicas
- **D**: Dependency Inversion - depende de abstracciones, no implementaciones

### 3. Capa de Infraestructura (Infrastructure Layer)

#### Implementación Kubernetes

```rust
struct KubernetesLifecycleManager {
    client: Client,
    namespace: String,
    provider_id: ProviderId,
    event_publisher: Arc<dyn LifecycleEventPublisher>
}
```

**Características**:
- ✅ **TTL Automático**: Pods se limpian automáticamente con `ttl_seconds_after_finished`
- ✅ **Active Deadline**: `active_deadline_seconds` previene workers colgados
- ✅ **Orphan Detection**: Detecta y limpia pods huérfanos automáticamente
- ✅ **Graceful Shutdown**: Terminación ordenada con grace period

## Flujo de Lifecycle

### 1. Creación de Worker
```
1. create_worker() llamado
2. Provider crea worker físico (Pod/Container)
3. LifecycleManager.initialize_worker() - registra en tracking
4. LifecycleEvent: WorkerCreated publicado
```

### 2. Worker en Funcionamiento
```
1. get_worker_status() - sync provider state con lifecycle state
2. mark_worker_ready() cuando está listo
3. mark_worker_idle() cuando no tiene jobs
```

### 3. Terminación
```
1. destroy_worker() llamado
2. LifecycleManager.start_termination() - marca Terminating
3. Provider destruye worker físico
4. LifecycleManager.mark_worker_terminated() - marca Terminated
5. Si auto_cleanup_enabled: programa cleanup con TTL
```

### 4. Garbage Collection
```
1. GC Cycle ejecutado periódicamente
2. Para cada worker en estado Terminated:
   a. Verificar si TTL expirado
   b. Si expirado: cleanup_worker()
   c. Eliminar pod de Kubernetes
   d. Remover de tracking interno
3. Detectar orphans (pods sin tracking)
4. Cleanup orphans inmediatamente
5. LifecycleEvent: WorkerCleanedUp publicado
```

## Beneficios del Diseño

### ✅ Workers Verdaderamente Efímeros
- Auto-cleanup basado en TTL
- Terminación forzada por `active_deadline_seconds`
- Limpieza inmediata en caso de falla

### ✅ Limpieza Automática y Ordenada
- GC automático con configuración configurable
- Orphan detection y cleanup
- Graceful shutdown con timeout

### ✅ Resiliencia ante Fallas
- Si provider falla, lifecycle manager continúa
- Events publicados para auditoría
- Force cleanup disponible

### ✅ Extensibilidad
- Pattern Decorator permite añadir a cualquier provider
- Interfaces abstratas para todos los providers
- Implementación específica por provider

### ✅ Observabilidad
- Events de lifecycle para auditoría
- Métricas de GC (workers cleaned, orphans detected)
- Logs estructurados para debugging

## Configuración Recomendada

### Para Kubernetes
```rust
EphemeralWorkerPolicy {
    max_lifetime: 1 hour,
    ttl_after_completion: 5 minutes,
    idle_timeout: 10 minutes,
    auto_cleanup_enabled: true,
    orphan_detection_enabled: true,
    orphan_check_interval: 1 minute
}
```

### Para Docker
```rust
EphemeralWorkerPolicy {
    max_lifetime: 30 minutes,
    ttl_after_completion: 2 minutes,
    idle_timeout: 5 minutes,
    auto_cleanup_enabled: true,
    orphan_detection_enabled: false,
    orphan_check_interval: 5 minutes
}
```

## Plan de Implementación

### Fase 1: Interfaces de Dominio
- [x] Definir `LifecycleState`, `LifecycleEvent`
- [x] Crear `EphemeralWorkerPolicy`
- [x] Interfaces `WorkerLifecycleManager`, `GarbageCollector`
- [x] Interface `LifecycleEventPublisher`

### Fase 2: Capa de Aplicación
- [x] Implementar `LifecycleManagedWorkerProvider` (Decorator)
- [x] Builder para configuración
- [x] Tests unitarios del adapter

### Fase 3: Implementación Kubernetes
- [x] `KubernetesLifecycleManager`
- [x] Integración con Kubernetes API
- [x] Orphan detection
- [x] TTL y active_deadline_seconds en pod spec

### Fase 4: Integración
- [ ] Integrar en `main.rs`
- [ ] Configurar lifecycle para cada provider
- [ ] Probar end-to-end

### Fase 5: Extensiones
- [ ] Implementación para Docker provider
- [ ] Métricas Prometheus
- [ ] Alertas por orphans

## Migración

### Backward Compatibility
- ✅ Decorator pattern permite añadir sin breaking changes
- ✅ Providers existentes siguen funcionando
- ✅ Opt-in para lifecycle management

### Rollback
- Si hay problemas, simplemente remover decorator
- Providers vuelven a comportamiento original

## Conclusión

Este diseño proporciona un sistema robusto, extensible y mantenible para gestión de workers efímeros, siguiendo las mejores prácticas de arquitectura de software y DDD.

---

**Principios Clave**:
1. **SOLID**: Single Responsibility, Open/Closed, Liskov, Interface Segregation, Dependency Inversion
2. **DDD**: Domain Events, Aggregate Pattern, Event Sourcing
3. **Clean Architecture**: Separación clara de responsabilidades por capas
4. **Pattern Decorator**: Extensibilidad sin modificación de código existente
