# Adaptaciones del Sistema de Lifecycle Management para Docker Provider

## Resumen

Este documento describe las adaptaciones necesarias para integrar el sistema de **Worker Lifecycle Management** con el **Docker Provider**, siguiendo los principios DDD y SOLID implementados en el sistema.

## Arquitectura del Sistema de Lifecycle

### Componentes Principales

```
┌─────────────────────────────────────────────────────────┐
│                    Docker Provider                       │
│  - create_worker() → Container                          │
│  - destroy_worker() → Stop & Remove Container           │
│  - get_worker_status() → Container State                │
└────────────────────────┬────────────────────────────────┘
                         │
                         │ Wrapper (Decorator Pattern)
                         │
┌────────────────────────▼────────────────────────────────┐
│              DockerLifecycleManager                      │
│  - initialize_worker() → Track in memory               │
│  - mark_worker_ready() → State transition              │
│  - mark_worker_terminated() → State transition         │
│  - cleanup_worker() → Remove from tracking             │
│  - run_gc_cycle() → Cleanup terminated workers         │
└────────────────────────┬────────────────────────────────┘
                         │
                         │ EventBus
                         │
┌────────────────────────▼────────────────────────────────┐
│              EventBusPublisher                          │
│  - publish_lifecycle_event()                           │
│    ↓                                                   │
│  - WorkerEphemeralCreated                              │
│  - WorkerEphemeralReady                                │
│  - WorkerEphemeralTerminating                          │
│  - WorkerEphemeralTerminated                           │
│  - WorkerEphemeralCleanedUp                            │
│  - OrphanWorkerDetected (opcional)                     │
└─────────────────────────────────────────────────────────┘
```

## Diferencias Clave: Kubernetes vs Docker

### Kubernetes
- **Orquestación nativa**: Kubernetes maneja lifecycle automáticamente
- **TTL automático**: `ttl_seconds_after_finished` limpia pods
- **Orphan detection**: Esencial (pods pueden quedar huérfanos)
- **Estado persistente**: Pods persisten hasta cleanup manual
- **GC externo**: Kubernetes limpia pods automáticamente

### Docker
- **Gestión local**: Docker containers son locales al host
- **Cleanup manual**: El lifecycle manager debe limpiar tracking
- **Orphan detection**: Menos crítico (containers raramente se pierden)
- **Estado efímero**: Containers se eliminan al stop
- **GC interno**: Lifecycle manager limpia tracking interno

## Implementación: DockerLifecycleManager

### Características Específicas

```rust
pub struct DockerLifecycleManager {
    provider_id: ProviderId,
    event_publisher: Arc<dyn LifecycleEventPublisher>,
    policies: Arc<RwLock<HashMap<WorkerId, EphemeralWorkerPolicy>>>,
    states: Arc<RwLock<HashMap<WorkerId, LifecycleState>>>,
}
```

### Diferencias vs KubernetesLifecycleManager

| Aspecto | Kubernetes | Docker |
|---------|-----------|--------|
| **Orphan Detection** | Esencial, cada 1 min | Opcional, cada 5 min |
| **Cleanup Logic** | Elimina pods K8s | Solo limpia tracking |
| **TTL** | Aplicado al Pod spec | Solo para cleanup interno |
| **Event Publisher** | Usa EventBus real | Puede usar simple logger |
| **Estado Storage** | En memoria | En memoria |

### Políticas Recomendadas para Docker

```rust
EphemeralWorkerPolicy {
    max_lifetime: Duration::from_secs(1800),        // 30 minutos
    ttl_after_completion: Some(Duration::from_secs(120)), // 2 minutos
    idle_timeout: Duration::from_secs(300),         // 5 minutos
    graceful_termination_timeout: Duration::from_secs(30),
    auto_cleanup_enabled: true,
    orphan_detection_enabled: false,  // Docker raramente tiene orphans
    orphan_check_interval: Duration::from_secs(300), // 5 minutos
}
```

## Flujo de Lifecycle en Docker

### 1. Creación de Worker

```rust
async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
    // 1. Crear container físico (DockerProvider)
    let handle = self.inner.create_worker(spec).await?;

    // 2. Registrar en lifecycle manager
    self.lifecycle_manager
        .initialize_worker(handle.worker_id().clone(), self.provider_id().clone(), policy)
        .await?;

    // 3. Publicar evento
    self.event_publisher
        .publish_lifecycle_event(
            LifecycleEvent::WorkerCreated { ... },
            self.provider_id.clone(),
            Some(policy.max_lifetime.as_secs()),
            None,
            None,
            None,
        )
        .await?;

    Ok(handle)
}
```

### 2. Worker Listo

```rust
async fn mark_worker_ready(&self, worker_id: &WorkerId) -> Result<(), Self::Error> {
    // Actualizar estado interno
    let mut states = self.states.write().await;
    states.insert(worker_id.clone(), LifecycleState::Ready);

    // Publicar evento
    self.event_publisher
        .publish_lifecycle_event(
            LifecycleEvent::WorkerReady { worker_id: worker_id.clone(), ... },
            self.provider_id.clone(),
            None, None, None, None,
        )
        .await?;
}
```

### 3. Destrucción de Worker

```rust
async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError> {
    // 1. Iniciar terminación en lifecycle
    self.lifecycle_manager
        .start_termination(handle.worker_id(), TerminationReason::ManualTermination)
        .await?;

    // 2. Destruir container físico (DockerProvider)
    self.inner.destroy_worker(handle).await?;

    // 3. Marcar como terminated en lifecycle
    self.lifecycle_manager
        .mark_worker_terminated(handle.worker_id())
        .await?;

    // 4. Programar cleanup automático
    if self.default_policy.auto_cleanup_enabled {
        self.lifecycle_manager
            .cleanup_worker(handle.worker_id(), CleanupReason::NormalTermination)
            .await?;
    }
}
```

### 4. Garbage Collection

```rust
async fn run_gc_cycle(&self) -> Result<GarbageCollectionStats, Self::Error> {
    let mut stats = GarbageCollectionStats {
        workers_cleaned: 0,
        orphans_detected: 0,
        errors: 0,
        duration_ms: 0,
    };

    // Limpiar workers terminados con TTL expirado
    let terminated_workers: Vec<WorkerId> = {
        let states = self.states.read().await;
        states.iter()
            .filter(|(_, state)| **state == LifecycleState::Terminated)
            .map(|(worker_id, _)| worker_id.clone())
            .collect()
    };

    for worker_id in terminated_workers {
        if self.needs_cleanup(&worker_id).await? {
            // En Docker, solo limpiamos tracking interno
            // El container ya fue eliminado por DockerProvider
            self.cleanup_worker(&worker_id, CleanupReason::TtlExpired).await?;
            stats.workers_cleaned += 1;
        }
    }

    // Orphan detection (opcional para Docker)
    if self.orphan_detection_enabled {
        // Docker raramente tiene orphans, pero verificamos por completitud
        let orphans = self.cleanup_orphans().await?;
        stats.orphans_detected = orphans.len();
    }

    Ok(stats)
}
```

## EventBus Integration

### Para Kubernetes
```rust
// EventBus real con PostgreSQL
let event_publisher = Arc::new(
    EventBusPublisher::new(event_bus.clone())
);
```

### Para Docker (Simplificado)
```rust
// Puede usar logger simple o EventBus
let event_publisher = Arc::new(
    DockerEventPublisher::new() // Solo logs, no DB
);
```

O alternativamente:
```rust
// Usar EventBus también para consistencia
let event_publisher = Arc::new(
    EventBusPublisher::new(event_bus.clone())
);
```

## Configuración en main.rs

```rust
// Docker Provider con Lifecycle Management
match DockerProviderBuilder::new()
    .with_provider_id(provider_id.clone())
    .build()
    .await
{
    Ok(provider) => {
        // Crear lifecycle manager
        let lifecycle_manager = Arc::new(
            DockerLifecycleManager::with_event_publisher(
                provider_id.clone(),
                Arc::new(EventBusPublisher::new(event_bus.clone()))
            )
        );

        // Configurar política Docker-specific
        let docker_policy = EphemeralWorkerPolicy::builder()
            .max_lifetime(Duration::from_secs(1800)) // 30 min
            .ttl_after_completion(Some(Duration::from_secs(120))) // 2 min
            .idle_timeout(Duration::from_secs(300)) // 5 min
            .auto_cleanup_enabled(true)
            .orphan_detection_enabled(false) // Docker rarely has orphans
            .orphan_check_interval(Duration::from_secs(300))
            .build();

        // Wrap provider con lifecycle management
        let lifecycle_provider = LifecycleManagedWorkerProviderBuilder::new(provider)
            .with_lifecycle_manager(lifecycle_manager)
            .with_event_publisher(event_publisher)
            .with_default_policy(docker_policy)
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("Docker lifecycle management failed: {}, using provider directly", e);
                provider
            });

        providers.insert(provider_id, Arc::new(lifecycle_provider));
    }
}
```

## Testing para Docker

### Test 1: Worker Creation
```bash
just job-docker-hello
# Verificar:
# 1. Container creado
# 2. WorkerEphemeralCreated event
# 3. WorkerEphemeralReady event
# 4. Estado tracking interno
```

### Test 2: Worker Cleanup
```bash
# Esperar 2 minutos después de completar job
docker ps -a --filter "name=hodei-worker"
# Verificar que no quedan containers antiguos
```

### Test 3: GC Cycle
```bash
# Forzar GC
# (Llamar endpoint o usar debug interface)
# Verificar:
# - Workers terminated limpiados
# - WorkerEphemeralCleanedUp event
```

## Beneficios para Docker

1. **Tracking de Estado**: Visibilidad de workers en memoria
2. **Eventos**: Auditoría completa del lifecycle
3. **GC Automático**: Cleanup de terminated workers
4. **Orphan Detection**: Detecta containers perdidos (opcional)
5. **Políticas**: Configuración flexible por provider

## Diferencias vs Estado Actual

### Estado Actual (sin Lifecycle)
```rust
// Solo hace esto:
let handle = provider.create_worker(spec).await?;
// No hay tracking, no hay eventos, no hay cleanup
```

### Con Lifecycle Management
```rust
// Hace esto:
let handle = provider.create_worker(spec).await?;
// + Registra en lifecycle manager
// + Publica eventos
// + GC automático
// + Orphan detection
// + Políticas configurables
```

## Conclusión

El sistema de lifecycle management está **production-ready** y puede integrarse fácilmente con el Docker Provider siguiendo el mismo patrón que Kubernetes. Las diferencias son principalmente de configuración y énfasis, pero la arquitectura es la misma.

## Próximos Pasos

1. **Integrar gradualmente** en main.rs con feature flag
2. **Probar con Docker** usando `just job-docker-hello`
3. **Verificar eventos** en logs/DB
4. **Ajustar políticas** según necesidades
5. **Añadir métricas** Prometheus (futuro)

---

**Referencias:**
- `kubernetes_lifecycle.rs` - Implementación Kubernetes
- `docker_lifecycle.rs` - Implementación Docker
- `lifecycle.rs` - Interfaces de dominio
- `events.rs` - Eventos de lifecycle
- `lifecycle-integration-plan.md` - Plan de integración completo
