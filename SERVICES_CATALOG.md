# Catálogo de Servicios y Sistemas - Hodei Job Platform
**Fecha:** 2025-12-27  
**Basado en:** Análisis del código fuente
---
## Índice
1. [Visión General](#visión-general)
2. [Domain Layer Services](#domain-layer-services)
3. [Application Layer Services](#application-layer-services)
4. [Infrastructure Layer Components](#infrastructure-layer-components)
5. [Interface Layer Adapters](#interface-layer-adapters)
6. [Dependencias entre Servicios](#dependencias-entre-servicios)
7. [Matriz de Responsabilidades](#matriz-de-responsabilidades)
---
## 1. Visión General
El sistema Hodei Job Platform está compuesto por **4 capas principales** con servicios especializados en cada una.
---
## 2. Domain Layer Services
### 2.1 Job Aggregate
| Atributo | Valor |
|----------|-------|
| **Ubicación** | crates/server/domain/src/jobs/aggregate.rs |
| **Tipo** | Aggregate Root |
| **Responsabilidad** | Gestionar el ciclo de vida completo de un job |
| **Bounded Context** | Jobs |
**Funcionalidades:**
- Creación de jobs con especificación validada
- Transiciones de estado (State Machine)
- Asignación a providers
- Tracking de intentos y resultados
### 2.2 Worker Aggregate
| Atributo | Valor |
|----------|-------|
| **Ubicación** | crates/server/domain/src/workers/aggregate.rs |
| **Tipo** | Aggregate Root |
| **Responsabilidad** | Representar workers efímeros y su estado |
| **Bounded Context** | Workers |
### 2.3 SmartScheduler
| Atributo | Valor |
|----------|-------|
| **Ubicación** | crates/server/domain/src/scheduling/mod.rs |
| **Tipo** | Domain Service |
| **Responsabilidad** | Selección inteligente de workers y providers |
**Estrategias Soportadas:**
- LeastLoaded, FirstAvailable, RoundRobin (workers)
- FastestStartup, LowestCost, HealthiestProvider, MostCapacity (providers)
### 2.4 Repository Traits
| Trait | Ubicación | Aggregate |
|-------|-----------|-----------|
| JobRepository | domain/src/jobs/aggregate.rs | Job |
| JobQueue | domain/src/jobs/aggregate.rs | Job |
| WorkerRegistry | domain/src/workers/registry.rs | Worker |
| AuditRepository | domain/src/audit/model.rs | AuditLog |
| OutboxRepository | domain/src/outbox/repository.rs | OutboxEvent |
| ProviderConfigRepository | domain/src/providers/config.rs | ProviderConfig |
---
## 3. Application Layer Services
### 3.1 Use Cases
| Use Case | Archivo | Responsabilidad |
|----------|---------|-----------------|
| CreateJobUseCase | jobs/create.rs | Crear nuevo job |
| CancelJobUseCase | jobs/cancel.rs | Cancelar job |
| RetryJobUseCase | jobs/create.rs | Reintentar job |
### 3.2 Controllers/Orchestrators
| Servicio | Archivo | Patrón |
|----------|---------|--------|
| JobController | jobs/controller.rs | Facade |
| JobCoordinator | jobs/coordinator.rs | Orchestrator |
| JobDispatcher | jobs/dispatcher.rs | Dispatcher |
| WorkerLifecycleManager | workers/lifecycle.rs | Manager |
| ProviderRegistry | providers/registry.rs | Registry |
| SchedulingService | scheduling/smart_scheduler.rs | Wrapper |
---
## 4. Infrastructure Layer Components
### 4.1 Persistence (PostgreSQL)
| Implementación | Trait Implementado |
|----------------|-------------------|
| PostgresJobRepository | JobRepository |
| PostgresJobQueue | JobQueue |
| PostgresWorkerRegistry | WorkerRegistry |
| PostgresAuditRepository | AuditRepository |
### 4.2 Worker Providers
| Provider | Archivo | Tecnología |
|----------|---------|------------|
| DockerProvider | providers/docker.rs | bollard |
| KubernetesProvider | providers/kubernetes.rs | kube-rs |
| FirecrackerProvider | providers/firecracker.rs | Firecracker |
### 4.3 Messaging
| Componente | Archivo | Propósito |
|------------|---------|-----------|
| OutboxEventBus | messaging/outbox_adapter.rs | Adapter EventBus->Outbox |
| OutboxRelay | messaging/outbox_relay/ | Polling y publicación |
---
## 5. Interface Layer Adapters
### 5.1 gRPC Services
| Servicio | Archivo | Proto |
|----------|---------|-------|
| JobExecutionServiceImpl | grpc/job_execution.rs | job_execution.proto |
| WorkerAgentServiceImpl | grpc/worker.rs | worker_agent.proto |
| SchedulerServiceImpl | grpc/scheduler.rs | scheduler.proto |
| AuditServiceImpl | grpc/audit.rs | audit.proto |
| ProviderManagementServiceImpl | grpc/provider_management.rs | provider_management.proto |
---
## 6. Dependencias entre Servicios
### 6.1 Diagrama de Dependencias
gRPC Services -> Application Layer -> Domain Layer <- Infrastructure Layer
### 6.2 Dependencias por Componente
| Componente | Depende de |
|------------|-----------|
| CreateJobUseCase | JobRepository, JobQueue, EventBus |
| JobDispatcher | JobQueue, JobRepository, WorkerRegistry, ProviderRegistry, EventBus |
| WorkerLifecycleManager | WorkerRegistry, WorkerProvider[], EventBus, WorkerHealthService |
| ProviderRegistry | ProviderConfigRepository, EventBus |
---
## 7. Matriz de Responsabilidades
### Flujo de Datos Principal
1. Cliente envía QueueJobRequest via gRPC
2. JobExecutionServiceImpl valida y mapea a DTO
3. CreateJobUseCase crea Job aggregate
4. JobRepository.save() persiste + encola atómicamente
5. EventBus publica JobCreated
6. JobDispatcher obtiene job de cola
7. SmartScheduler selecciona provider
8. WorkerLifecycleManager provisiona worker
9. Provider crea container/pod/VM
10. Worker se registra via WorkerAgentService
11. JobDispatcher envía job a worker
12. Worker ejecuta y reporta resultado
13. Job.complete() actualiza estado
14. EventBus publica JobStatusChanged
15. WorkerLifecycleManager termina worker efímero
---
*Documento generado el 2025-12-27 basado en análisis del código fuente*
