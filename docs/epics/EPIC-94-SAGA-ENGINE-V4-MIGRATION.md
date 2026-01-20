# EPIC-94: Migración Directa a Saga Engine v4.0 (Durable Execution)

> **Estado**: En Ejecución | **Dependencias**: EPIC-93 (saga-engine v4.0 library) | **Prioridad**: Crítica
> **Última Actualización**: 2026-01-20 | **Progreso Total**: 68%

## 🎯 Objetivo de la Épica

Migrar el sistema de orquestación de sagas a la nueva arquitectura **Durable Execution** basada en la librería `saga-engine v4.0`. Este cambio elimina la deuda técnica del sistema embebido actual y proporciona fiabilidad industrial mediante NATS JetStream y PostgreSQL Event Sourcing.

---

## 📊 Dashboard de Progreso

```
    ╔══════════════════════════════════════════════════════════════════════════════╗
    ║                   EPIC-94 V4.0 DIRECT MIGRATION DASHBOARD                    ║
    ╠══════════════════════════════════════════════════════════════════════════════╣
    ║                                                                              ║
    ║  ███████████████████████████████████████████████████████████████░░░░░░░░░░░  68% ║
    ║                                                                              ║
    ║  Fase 1: Infrastructure Durability  ████████████████████████████  100% ✅    ║
    ║  Fase 2: Core Engine Components     ████████████████████████████   95% 🔄    ║
    ║  Fase 3: Native Workflow Porting    ████████████░░░░░░░░░░░░░░░░   40% 🔄    ║
    ║  Fase 4: System Integration         ░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0% ⏳    ║
    ║  Fase 5: Legacy Decommissioning     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0% ⏳    ║
    ║                                                                              ║
    ║  Performance Indicators:                                                       ║
    ║  ┌─────────────────┬────────────┬────────────┬────────────┐                  ║
    ║  │ Replay Latency  │ Task Loss  │ Snapshot HP│ v4 Uptime  │                  ║
    ║  │ < 50ms (Target) │ 0% ✅      │ ✅ Ready   │ N/A        │                  ║
    ║  └─────────────────┴────────────┴────────────┴────────────┘                  ║
    ║                                                                              ║
    ╚══════════════════════════════════════════════════════════════════════════════╝
```

---

## 📋 User Stories Status

### Fase 1: Infrastructure Durability ✅ COMPLETADA

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.1** | NATS JetStream Task Queue | ✅ Done | Platform Team | Reemplaza Pub/Sub con Pull Consumers |
| **US-94.2** | PostgreSQL History Replayer | ✅ Done | Platform Team | Reconstrucción de estado vía Event Sourcing |
| **US-94.3** | Atomic Event Appends | ✅ Done | Platform Team | Optimistic locking en `EventStore` |

### Fase 2: Core Engine Components 🔄 EN PROGRESO

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.4** | Durable Workflow Executor | ✅ Done | Platform Team | Bucle principal de ejecución con métricas |
| **US-94.5** | Snapshot Management | ✅ Done | Platform Team | Integrado en Replayer y Executor |
| **US-94.6** | Type-Safe SagaPort | ✅ Done | Platform Team | Port genérico sobre `WorkflowDefinition` |
| **US-94.14** | Workflow Metrics & Tracing | ✅ Done | Platform Team | **NUEVA** - Métricas de observabilidad |
| **US-94.15** | Enhanced WorkflowContext | ✅ Done | Platform Team | **NUEVA** - Signals y cancelación |

### Fase 3: Native Workflow Porting 🔄 EN PROGRESO

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.7** | Recovery Workflow Porting | 🔄 In-Progress | Platform Team | Redefinición nativa en v4 |
| **US-94.8** | Provisioning Workflow Porting | ⏳ Pending | - | Migración de actividades a v4 |
| **US-94.9** | Execution Workflow Porting | ⏳ Pending | - | Migración de actividades a v4 |

### Fase 4: System Integration ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.10** | Native gRPC Orchestration | ⏳ Pending | - | `SchedulerServiceImpl` usa v4 directamente |
| **US-94.11** | Universal Signal Bridge | ⏳ Pending | - | Conexión de Domain Events a v4 Signals |

### Fase 5: Legacy Decommissioning ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.12** | Embedded Engine Deletion | ⏳ Pending | - | Eliminación del orquestador legacy |
| **US-94.13** | Database Migration (Cleanup) | ⏳ Pending | - | Drop de tablas `sagas` y `saga_steps` legacy |

### Fase 6: Resilience Ports (Semanas 11-12) ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.19** | Legacy Rate Limiter Port | ⏳ Pending | - | **NUEVA** |
| **US-94.20** | Legacy Circuit Breaker Port | ⏳ Pending | - | **NUEVA** |
| **US-94.21** | Legacy Stuck Detection Port | ⏳ Pending | - | **NUEVA** |

### Fase 7: Cutover Preparation (Semanas 13-14) ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.9** | Feature Flag para Dual-Write | ⏳ Pending | - | - |
| **US-94.10** | Migration Runner | ⏳ Pending | - | - |

### Fase 8: Legacy Cleanup (Semanas 15-18) ⏳ PENDIENTE

| US | Título | Estado | owner | Notas |
|----|--------|--------|-------|-------|
| **US-94.11** | Cleanup de Código Legacy | ⏳ Pending | - | - |
| **US-94.18** | Safe Legacy Code Deletion Strategy | ⏳ Pending | - | **NUEVA** |

---

## 📖 Contexto y Motivación

### Estado Actual del Sistema Saga

La implementación actual de saga en Hodei Jobs es **production-ready** e incluye:

| Aspecto | Implementación Actual |
|---------|----------------------|
| **Patrón Saga** | Embebido, con compensación automática |
| **Event Sourcing** | Completo (SagaEventStore, EventSourcedSagaState) |
| **Tipos de Saga** | 6 workflows (Provisioning, Execution, Recovery, Cancellation, Timeout, Cleanup) |
| **Persistencia** | PostgreSQL con SQLx |
| **Mensajería** | NATS con consumidores reactivos |
| **Resiliencia** | Circuit breaker, rate limiting, reintentos |
| **Tipos de Sagas** | ProvisioningSaga, ExecutionSaga, RecoverySaga, CancellationSaga, TimeoutSaga, CleanupSaga |

### Limitaciones del Enfoque Actual

1. **Acoplamiento fuerte** entre dominio e infraestructura
2. **Difícil test** de sagas de forma aislada (requiere PostgreSQL + NATS)
3. **Sin portabilidad** del código saga a otros proyectos
4. **Mantenimiento monolítico** de toda la infraestructura saga

---

## 🏗️ Arquitectura de Migración (Directa)

### Principios de Diseño

```
    ┌─────────────────────────────────────────────────────────────────┐
    ║                    V4.0 DIRECT INTEGRATION                      ║
    ╠═════════════════════════════════════════════════════════════════╣
    ║                                                                 ║
    ║  ┌─────────────────────────────────────────────────────────┐   ║
    ║  │                  APPLICATION LAYER                       │   ║
    ║  │  ┌──────────────────────────────────────────────────┐   │   ║
    ║  │  │         SagaPort<W: WorkflowDefinition>          │   │   ║
    ║  │  │      (Strongly Typed Input/Output Interface)     │   │   ║
    ║  │  └──────────────────────────────────────────────────┘   │   ║
    ║  └─────────────────────────────────────────────────────────┘   ║
    ║                         ↓ (Direct Call)                      ║
    ║  ┌─────────────────────────────────────────────────────────┐   ║
    ║  │                SAGA ENGINE V4.0 LIBRARY                  │   ║
    ║  │  ┌──────────────────────────────────────────────────┐   │   ║
    ║  │  │            Durable Workflow Executor              │   │   ║
    ║  │  │  (JetStream Pull Consumer + Replay Loop)         │   │   ║
    ║  │  └──────────────────────────────────────────────────┘   │   ║
    ║  └─────────────────────────────────────────────────────────┘   ║
    ║          ↓                     ↓                     ↓         ║
    ║  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ ║
    ║  │  Event Store    │  │   Task Queue    │  │   Timer Store   │ ║
    ║  │  (PostgreSQL)   │  │ (NATS JetStream)│  │  (PostgreSQL)   │ ║
    ║  └─────────────────┘  └─────────────────┘  └─────────────────┘ ║
    ║                                                                 ║
    ╚═════════════════════════════════════════════════════════════════╝
```

### Estrategia de Migración: Direct Cutover

A diferencia de la estrategia original de dual-write, se opta por un **Direct Cutover** por tipo de workflow, aprovechando la madurez de la librería v4 desarrollada:

1. **Side-by-Side Running**: Los nuevos workflows se inician directamente en v4.
2. **Legacy Drain**: Los workflows existentes en el motor legacy se dejan finalizar normalmente.
3. **No New Legacy**: Se bloquea la creación de nuevos tipos de sagas en el motor antiguo.

---

## 📋 Fases de Implementación Reformuladas

### Fase 1: Infrastructure Durability ✅ COMPLETADA
Foco en asegurar que NATS JetStream y PostgreSQL manejen la durabilidad de forma industrial.

### Fase 2: Core Engine Components 🔄 EN PROGRESO
Implementación del Replayer, Snapshot Manager y el loop principal del Executor.

### Fase 3: Native Workflow Porting 🔄 EN PROGRESO
Portar `Recovery`, `Provisioning` y `Execution` a la estructura nativa de v4.

### Fase 4: System Integration ⏳ PENDIENTE
Conectar los servicios de aplicación (`Scheduler`, `JobService`) directamente al `SagaPort` de v4.

---

## 🧪 Strategy de Testing (V4 Focused)

### Test Pyramid

```
    ┌─────────────────────────────────────────────────────────────┐
    │                    V4 TEST PYRAMID                          │
    ├─────────────────────────────────────────────────────────────┤
    │                                                             │
    │                    ┌───────────┐                             │
    │                    │  E2E      │  ← Workflow Acceptance      │
    │                    │  Tests    │    (Real NATS + Postgres)   │
    │                    └───────────┘                             │
    │                   ┌───────────────┐                          │
    │                  │  Integration  │ ← Port/Adapter Tests      │
    │                  │  Tests        │   (Activity isolation)    │
    │                  └───────────────┘                           │
    │                 ┌─────────────────┐                          │
    │                │   Unit Tests    │ ← Deterministic Replay    │
    │                │                 │   (Pure logic tests)      │
    │                └─────────────────┘                           │
    │                                                              │
    └─────────────────────────────────────────────────────────────┘
```

---

## 🚨 Gestión de Riesgos Actualizada

| ID | Riesgo | Prob. | Imp. | Mitigación |
|----|--------|------|------|------------|
| **R-01** | Replay Performance | Media | Alta | Benchmarks y snapshots obligatorios |
| **R-02** | Task Queue Poisoning | Baja | Alta | Configuración de DLQ en JetStream |
| **R-03** | Distributed Lock Contention | Media | Media | Optimistic locking con versionado en EventStore |

---

## ✅ Definition of Done (Criterios Finales)

- [ ] Todos los workflows críticos (`Execution`, `Provisioning`, `Recovery`) ejecutándose en v4.
- [ ] 0% pérdida de tareas tras reinicio forzado de servicios (Worker/NATS).
- [ ] Latencia de replay de historial < 50ms para workflows de < 100 eventos.
- [ ] Código legacy del orquestador embebido eliminado completamente.
- [ ] Documentación técnica actualizada reflejando la arquitectura de orquestación única.

---

**Creado**: 2026-01-19 | **Última actualización**: 2026-01-20 | **Owner**: Platform Team
