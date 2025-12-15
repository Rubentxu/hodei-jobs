# ÉPICA 5: Scheduler Inteligente

## Estado: ✅ COMPLETADO

## Resumen
Sistema de scheduling inteligente con estrategias configurables para asignación de jobs a workers y selección de providers.

---

## Historias de Usuario Implementadas

### HU-5.1: JobScheduler trait y estrategias ✅
- [x] Trait `JobScheduler` con método `schedule()`
- [x] `SchedulingDecision` enum (AssignToWorker, ProvisionWorker, Enqueue, Reject)
- [x] `SchedulingContext` con toda la información para decidir
- [x] Enums para estrategias: `WorkerSelectionStrategy`, `ProviderSelectionStrategy`

### HU-5.2: Selectores con algoritmos ✅
- [x] `FirstAvailableWorkerSelector`
- [x] `LeastLoadedWorkerSelector`
- [x] `LowestCostProviderSelector`
- [x] `FastestStartupProviderSelector`
- [x] `MostCapacityProviderSelector`
- [x] `HealthiestProviderSelector`
- [x] Round-robin para workers y providers

### HU-5.3: SmartScheduler y JobOrchestrator ✅
- [x] `SmartScheduler` con configuración flexible
- [x] `SchedulingService` como facade
- [x] `JobOrchestrator` integrando scheduler + lifecycle

### HU-5.4: Tests ✅
- [x] Tests para selectores de providers
- [x] Tests para SmartScheduler
- [x] Tests para JobOrchestrator

---

## Archivos Creados

| Archivo | Descripción |
|---------|-------------|
| `crates/domain/src/job_scheduler.rs` | Traits y selectores |
| `crates/application/src/smart_scheduler.rs` | SmartScheduler |
| `crates/application/src/job_orchestrator.rs` | JobOrchestrator |

---

## Estrategias Disponibles

### Worker Selection
| Estrategia | Descripción |
|------------|-------------|
| `FirstAvailable` | Primer worker disponible |
| `LeastLoaded` | Worker con menos jobs ejecutados |
| `MostCapacity` | Worker con más recursos |
| `RoundRobin` | Distribución equitativa |
| `Affinity` | Afinidad por tipo de job |

### Provider Selection
| Estrategia | Descripción |
|------------|-------------|
| `FirstAvailable` | Primer provider con capacidad |
| `LowestCost` | Menor costo por hora |
| `FastestStartup` | Menor tiempo de arranque |
| `MostCapacity` | Mayor capacidad disponible |
| `RoundRobin` | Distribución equitativa |
| `Healthiest` | Mejor health score |

---

## API

```rust
// Configurar scheduler
let config = SchedulerConfig {
    worker_strategy: WorkerSelectionStrategy::LeastLoaded,
    provider_strategy: ProviderSelectionStrategy::FastestStartup,
    max_queue_depth: 100,
    scale_up_load_threshold: 0.8,
    prefer_existing_workers: true,
};

// Crear orchestrator
let orchestrator = JobOrchestrator::new(
    registry,
    job_repository,
    job_queue,
    config,
    lifecycle_config,
);

// Registrar provider
orchestrator.register_provider(docker_provider).await;

// Enviar job
let job_id = orchestrator.submit_job(job).await?;

// Ejecutar mantenimiento
let result = orchestrator.run_maintenance().await?;
```

---

## Flujo de Scheduling

```
┌─────────────┐    ┌─────────────────┐    ┌──────────────────┐
│  submit_job │───▶│ build_context   │───▶│ SmartScheduler   │
└─────────────┘    └─────────────────┘    └────────┬─────────┘
                                                   │
                   ┌───────────────────────────────┼───────────────────┐
                   │                               │                   │
                   ▼                               ▼                   ▼
           ┌──────────────┐              ┌─────────────────┐   ┌────────────┐
           │ AssignToWorker│              │ ProvisionWorker │   │ Enqueue    │
           └──────┬───────┘              └────────┬────────┘   └─────┬──────┘
                  │                               │                   │
                  ▼                               ▼                   ▼
           ┌──────────────┐              ┌─────────────────┐   ┌────────────┐
           │ Update State │              │ Create Worker   │   │ Add to     │
           │ Job = Running│              │ via Provider    │   │ Queue      │
           └──────────────┘              └─────────────────┘   └────────────┘
```

---

## Tests

```bash
cargo test -p hodei-jobs-domain job_scheduler
cargo test -p hodei-jobs-application smart_scheduler
cargo test -p hodei-jobs-application job_orchestrator
```

**Total: 120 tests pasando**

---

*Última actualización: 2025-12-11*
