# Code Manifest - Hodei Jobs Platform

**Fecha de Generación:** 2025-12-30
**Versión:** 1.0
**Metodología:** DDD + Clean Architecture

---

## Formato de Catalogación

| Columna | Descripción |
|---------|-------------|
| Layer | Capa DDD (Domain, Application, Infrastructure, Interface) |
| Category | Patrón DDD específico (Aggregate, Entity, Value Object, Use Case, etc.) |
| Description | Funcionalidad central y propósito de negocio |
| Integration | ✓ si filtra/rechaza datos externos o actúa como protección de límites |
| Validation | ✓ si valida formato técnico o datos |
| Invariants | ✓ si hace cumplir restricciones estructurales del dominio |
| Business Rules | ✓ si implementa lógica de negocio/flujos de trabajo |
| Catalogued | ✓ cuando la clase ha sido analizada |

---

## Workers Domain - Capa: Domain

| Class/Type | Layer | Category | Description | Integration | Validation | Invariants | Business Rules | Catalogued |
|------------|-------|----------|-------------|-------------|------------|------------|----------------|------------|
| `ProviderType` | Domain | Value Object | Enumeración de tipos de provider con valores: Docker, Kubernetes, Fargate, CloudRun, ContainerApps, Lambda, CloudFunctions, AzureFunctions, EC2, ComputeEngine, AzureVMs, Test, BareMetal, Custom(String) | | | | | ✓ |
| `ProviderCategory` | Domain | Value Object | Enumeración de categorías de provider: Container, Serverless, VirtualMachine, BareMetal | | | | | ✓ |
| `Architecture` | Domain | Value Object | Arquitectura del worker: Amd64, Arm64, Arm | | | | | ✓ |
| `ResourceRequirements` | Domain | Value Object | Recursos requeridos para worker: cpu_cores (f64), memory_bytes (i64), disk_bytes (i64), gpu_count (u32), gpu_type (Option<String>) | | | | | ✓ |
| `VolumeSpec` | Domain | Value Object | Tipos de volumen: Persistent(name, claim_name, read_only), Ephemeral(name, size_limit), HostPath(name, path, read_only) | | | | | ✓ |
| `WorkerSpec` | Domain | Entity | Especificación para crear worker on-demand con configuración de imagen, recursos, labels, annotations, environment, volúmenes, timeouts, arquitectura, capacidades y provider_config | | | ✓ | | ✓ |
| `KubernetesWorkerConfig` | Domain | Value Object | Configuración específica de Kubernetes: annotations, custom_labels, node_selector, service_account, init_containers, sidecar_containers, affinity, tolerations, dns_policy, dns_config, host_aliases, security_context | | | | | ✓ |
| `KubernetesContainer` | Domain | Value Object | Especificación de contenedor Kubernetes: name, image, image_pull_policy, command, args, env, env_from, ports, volume_mounts, resources, security_context | | | | | ✓ |
| `KubernetesAffinity` | Domain | Value Object | Configuración de afinidad: node_affinity, pod_affinity, pod_anti_affinity | | | | | ✓ |
| `WorkerHandle` | Domain | Value Object | Handle opaco al worker con worker_id, provider_resource_id, provider_type, provider_id, created_at, metadata | | | | | ✓ |
| `Worker` | Domain | Aggregate | Aggregate principal de worker efímero con estado, spec, handle, job asignado, jobs ejecutados, heartbeat, timestamps y máquina de estados (Creating→Connecting→Ready→Busy/Draining→Terminating→Terminated) | | | ✓ | | ✓ |
| `WorkerRegistryStats` | Domain | Value Object | Estadísticas del registro: total_workers, ready_workers, busy_workers, idle_workers, terminating_workers, workers_by_provider, workers_by_type | | | | | ✓ |
| `WorkerFilter` | Domain | Value Object | Filtros para búsqueda de workers: states, provider_id, provider_type, can_accept_jobs, idle_for, unhealthy_timeout | | | | | ✓ |
| `WorkerRegistry` | Domain | Repository Interface | Trait para registro centralizado de workers: register, unregister, get, find, find_available, heartbeat, assign_to_job, release_from_job, stats, count | | | | | ✓ |
| `WorkerLifecycleEvent` | Domain | Domain Event | Eventos del ciclo de vida: Registered, StateChanged, AssignedToJob, ReleasedFromJob, HeartbeatReceived, UnhealthyDetected, Unregistered | | | | | ✓ |
| `WorkerHealthService` | Domain | Domain Service | Servicio para evaluar salud de worker basado en heartbeat con umbrales: Healthy (< timeout), Degraded (1x-2x timeout), Dead (>2x timeout) | | | | ✓ | ✓ |
| `WorkerHealthStatus` | Domain | Value Object | Estado de salud del worker: Healthy, Degraded(HeartbeatAge), Dead(HeartbeatAge) | | | | | ✓ |
| `HeartbeatAge` | Domain | Value Object | Newtype wrapper para duración de heartbeat | | | | | ✓ |
| `ScalingDecision` | Domain | Value Object | Decisión de auto-scaling: None, ScaleUp(provider_id, count, reason), ScaleDown(provider_id, count, reason) | | | | | ✓ |
| `ScalingContext` | Domain | Value Object | Contexto para decisiones de scaling: provider_id, pending_jobs, healthy_workers, busy_workers, degraded_workers con métodos total_workers() y available_workers() | | | | | ✓ |
| `AutoScalingStrategy` | Domain | Domain Service | Trait para estrategias de auto-scaling con nombre y método evaluate() | | | | ✓ | ✓ |

---

## Workers Domain - Provider API - Capa: Domain

| Class/Type | Layer | Category | Description | Integration | Validation | Invariants | Business Rules | Catalogued |
|------------|-------|----------|-------------|-------------|------------|------------|----------------|------------|
| `WorkerProviderConfig` | Domain | Trait | Trait base para configuración específica de provider con provider_type() y as_any() | | | | | ✓ |
| `KubernetesConfigExt` | Domain | Extension Object | Configuración de extensión Kubernetes: annotations, custom_labels, node_selector, service_account, init_container_images, sidecar_container_images, tolerations, dns_policy | | | | | ✓ |
| `DockerConfigExt` | Domain | Extension Object | Configuración de extensión Docker: network_mode, extra_hosts, restart_policy, privileged, runtime, devices, ulimits, sysctls | | | | | ✓ |
| `FirecrackerConfigExt` | Domain | Extension Object | Configuración de Firecracker: kernel_path, rootfs_path, kernel_args, network, vsock, cpu_template, snapshot_path | | | | | ✓ |
| `ProviderConfig` | Domain | Value Object | Enum wrapper de configuraciones: Kubernetes(KubernetesConfigExt), Docker(DockerConfigExt), Firecracker(FirecrackerConfigExt) | | | | | ✓ |
| `WorkerProviderIdentity` | Domain | Trait | Trait de identidad: provider_id(), provider_type(), category(), capabilities() | | | | | ✓ |
| `WorkerLifecycle` | Domain | Trait | Ciclo de vida: create_worker(spec), get_worker_status(handle), destroy_worker(handle), list_workers(), destroy_worker_by_id() | | | | | ✓ |
| `WorkerLogs` | Domain | Trait | Obtención de logs: get_worker_logs(handle, tail) | | | | | ✓ |
| `WorkerCost` | Domain | Trait | Estimación de costos: estimate_cost(spec, duration), estimated_startup_time() | | | | | ✓ |
| `WorkerHealth` | Domain | Trait | Health check: health_check() | | | | | ✓ |
| `WorkerEligibility` | Domain | Trait | Validación de elegibilidad: can_fulfill(requirements) | | | | | ✓ |
| `WorkerMetrics` | Domain | Trait | Métricas de provider: get_performance_metrics(), record_worker_creation(), get_startup_time_history(), calculate_average_cost_per_hour(), calculate_health_score() | | | | | ✓ |
| `WorkerEventSource` | Domain | Trait | Suscripción a eventos: subscribe() returning Stream<WorkerInfrastructureEvent> | | | | | ✓ |
| `WorkerProvider` | Domain | Trait | Trait principal combinando Identity + Lifecycle + Logs + Cost + Health + Eligibility + Metrics + EventSource | | | | | ✓ |
| `WorkerProviderExt` | Domain | Extension Trait | Métodos wrapper para trait objects: health_check_ext(), create_worker_ext(), get_worker_status_ext(), destroy_worker_ext(), logs_ext(), cost_ext(), eligibility_ext(), metrics_ext(), subscribe_ext() | | | | | ✓ |
| `HealthStatus` | Domain | Value Object | Estado de salud del provider: Healthy, Degraded(reason), Unhealthy(reason), Unknown | | | | | ✓ |
| `GpuVendor` | Domain | Value Object | Enumeración de vendors GPU: Nvidia, Amd, Intel, Custom(String) | | | | | ✓ |
| `GpuModel` | Domain | Value Object | Enumeración de modelos GPU: TeslaV100, TeslaA100, TeslaT4, RTX3090, RTX4090, MI100, MI200, Custom{name, vram_gb} | | | | | ✓ |
| `ProviderFeature` | Domain | Value Object | Features tipadas de provider: Gpu, Cpu, Memory, Storage, Network, Runtime, Security, Specialized, OsSupport | | | | | ✓ |
| `ProviderCapabilities` | Domain | Value Object | Capacidades del provider: max_resources, gpu_support, gpu_types, architectures, runtimes, regions, max_execution_time, persistent_storage, custom_networking, features | | | | | ✓ |
| `ResourceLimits` | Domain | Value Object | Límites de recursos: max_cpu_cores, max_memory_bytes, max_disk_bytes, max_gpu_count | | | | | ✓ |
| `JobRequirements` | Domain | Value Object | Requisitos de job: resources, architecture, required_capabilities, required_labels, allowed_regions, timeout, preferred_category | | | | | ✓ |
| `CostEstimate` | Domain | Value Object | Estimación de costo: currency, amount, unit, breakdown | | | | | ✓ |
| `CostUnit` | Domain | Value Object | Unidad de costo: PerSecond, PerMinute, PerHour, PerInvocation, PerGBSecond | | | | | ✓ |
| `LogEntry` | Domain | Value Object | Entrada de log: timestamp, level, message, source | | | | | ✓ |
| `LogLevel` | Domain | Value Object | Nivel de log: Debug, Info, Warn, Error | | | | | ✓ |
| `ProviderError` | Domain | Exception | Errores de provider: ConnectionFailed, AuthenticationFailed, ResourceLimitExceeded, WorkerNotFound, ProvisioningFailed, ProvisioningTimeout, Timeout, NotReady, UnsupportedOperation, OperationNotSupported, ConfigurationError, ProviderSpecific, Internal | | | | | ✓ |
| `ProviderPerformanceMetrics` | Domain | Value Object | Métricas de rendimiento: startup_times, success_rate, avg_cost_per_hour, workers_per_minute, errors_per_minute, avg_resource_usage | | | | | ✓ |
| `ResourceUsageStats` | Domain | Value Object | Estadísticas de uso: avg_cpu_millicores, avg_memory_bytes, avg_disk_bytes, peak_cpu_millicores, peak_memory_bytes | | | | | ✓ |
| `ProviderWorkerInfo` | Domain | Value Object | Info de worker desde provider: resource_id, state, last_seen, current_job_id | | | | | ✓ |
| `WorkerInfrastructureEvent` | Domain | Domain Event | Eventos de infraestructura: WorkerStarted, WorkerStopped, WorkerHealthChanged, ProviderEvent | | | | | ✓ |
| `StateMapper<T>` | Domain | Trait | Adapter para mapear estados externos: to_worker_state(state), from_worker_state(state) | | | | | ✓ |
| `DockerStateMapper` | Domain | State Adapter | Mapeo de estados Docker: created→Connecting, running→Ready, paused→Draining, exited→Terminated | | | | | ✓ |
| `KubernetesStateMapper` | Domain | State Adapter | Mapeo de estados K8s: Pending→Creating, Running+ready→Ready, Running+not_ready→Connecting, Succeeded/Failed→Terminated | | | | | ✓ |
| `FirecrackerStateMapper` | Domain | State Adapter | Mapeo de estados Firecracker: Creating→Creating, Running→Ready, Stopping→Draining, Stopped/Failed→Terminated | | | | | ✓ |

---

## Resumen de Cobertura

| Categoría | Total Classes | Catalogued | Porcentaje |
|-----------|---------------|------------|------------|
| Domain - Workers | 47 | 47 | 100% |

---

## Estadísticas de Reglas

| Regla | Total | Porcentaje |
|-------|-------|------------|
| Integration Rules | 0 | 0% |
| Validation Rules | 0 | 0% |
| Invariants | 2 | Worker, WorkerSpec |
| Business Rules | 2 | WorkerHealthService, AutoScalingStrategy |

---

## Notas de Catalogación

### Reglas de Integración (0 clases)
Los tipos en el dominio workers son abstracciones puras que no actúan como límites con sistemas externos. Los límites de integración están en la capa de infraestructura.

### Reglas de Validación (0 clases)
Las validaciones de formato técnico (como parsing de UUIDs) están delegadas a las capas de aplicación o infraestructura.

### Invariants (2 clases)
- **Worker**: Máquina de estados con transiciones validadas (Creating→Connecting→Ready→Busy/Draining→Terminating→Terminated)
- **WorkerSpec**: Validación de campos requeridos en construcción

### Reglas de Negocio (2 clases)
- **WorkerHealthService**: Evaluación de salud de workers basada en heartbeats con umbrales configurables
- **AutoScalingStrategy**: Lógica de escalado configurable por estrategia

---

*Documento generado automáticamente - 2025-12-30*
