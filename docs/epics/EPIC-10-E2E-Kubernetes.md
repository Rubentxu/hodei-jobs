# EPIC-10: Tests E2E con Stack Completo - Kubernetes Provider

**Versión**: 1.0  
**Fecha**: 2025-12-14  
**Estado**: Planificado  
**Prioridad**: Media  
**Dependencias**: EPIC-9 (reutiliza infraestructura común)

## Resumen Ejecutivo

Implementar tests End-to-End (E2E) que prueben el stack completo de Hodei Jobs Platform usando el Kubernetes Provider. Los workers se ejecutarán como Pods en un cluster Kubernetes real (Kind para desarrollo).

## Objetivos

1. Validar el flujo E2E completo con Kubernetes como provider
2. Verificar el provisioning de workers como Pods
3. Probar networking entre server (host) y workers (cluster)
4. Validar el ciclo de vida completo de Pods
5. Asegurar compatibilidad con clusters reales

## Arquitectura del Test

```
┌─────────────────────────────────────────────────────────────────┐
│                    TEST E2E KUBERNETES                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐     ┌──────────────────────────────────────┐  │
│  │  Testcontainers │   │         gRPC Server (Host)           │  │
│  │  PostgreSQL     │◄──┤  - WorkerAgentService                 │  │
│  │  :5432          │   │  - KubernetesProvider ✓               │  │
│  └──────────────┘     │  - Expuesto via NodePort/Ingress      │  │
│                        └──────────────┬───────────────────────┘  │
│                                       │                          │
│  ┌────────────────────────────────────┼──────────────────────┐  │
│  │            Kind Cluster            │                       │  │
│  │                                    ▼                       │  │
│  │  ┌──────────────────────────────────────────────────────┐ │  │
│  │  │  Namespace: hodei-workers                             │ │  │
│  │  │  ┌────────────────┐  ┌────────────────┐              │ │  │
│  │  │  │  Pod: worker-1 │  │  Pod: worker-2 │  ...         │ │  │
│  │  │  │  - Worker Agent│  │  - Worker Agent│              │ │  │
│  │  │  │  - HODEI_TOKEN │  │  - HODEI_TOKEN │              │ │  │
│  │  │  └────────────────┘  └────────────────┘              │ │  │
│  │  └──────────────────────────────────────────────────────┘ │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Historias de Usuario

### HU-10.1: Setup de Cluster Kind para Tests

**Como** desarrollador  
**Quiero** un cluster Kind configurado automáticamente para tests  
**Para** no depender de clusters externos

**Criterios de Aceptación:**
- [ ] Script crea cluster Kind si no existe
- [ ] Namespace `hodei-workers` creado
- [ ] RBAC configurado para crear/eliminar Pods
- [ ] Imagen worker cargada en cluster

**Tareas:**
- [ ] Crear `scripts/e2e/setup-kind-cluster.sh`
- [ ] Crear manifiestos RBAC en `deploy/kubernetes/rbac-test.yaml`
- [ ] Documentar requisitos de Kind

### HU-10.2: Exposición del Server al Cluster

**Como** desarrollador  
**Quiero** que el gRPC server sea accesible desde dentro del cluster  
**Para** que los workers puedan conectarse

**Criterios de Aceptación:**
- [ ] Server accesible via `host.docker.internal` (Kind en Docker)
- [ ] Alternativa: port-forward desde el cluster
- [ ] Configuración documentada

**Tareas:**
- [ ] Investigar opciones de networking Kind ↔ Host
- [ ] Implementar helper para obtener IP del host desde cluster
- [ ] Documentar configuración

### HU-10.3: Test E2E de Provisioning de Pod

**Como** desarrollador  
**Quiero** un test que verifique la creación de Pods worker  
**Para** validar el KubernetesProvider

**Criterios de Aceptación:**
- [ ] Job encolado trigger creación de Pod
- [ ] Pod aparece en namespace `hodei-workers`
- [ ] Pod tiene labels correctos
- [ ] Pod tiene env vars correctas (HODEI_TOKEN, HODEI_SERVER)

**Tareas:**
- [ ] Implementar `test_k8s_pod_provisioning()` en `e2e_kubernetes_provider.rs`
- [ ] Verificar Pod via kubectl/API
- [ ] Verificar estado del Pod

### HU-10.4: Test E2E de Ejecución de Job en Pod

**Como** desarrollador  
**Quiero** un test que ejecute un job completo en un Pod  
**Para** validar el flujo E2E

**Criterios de Aceptación:**
- [ ] Job se ejecuta dentro del Pod
- [ ] Logs llegan al servidor
- [ ] Job se completa exitosamente
- [ ] Pod se puede eliminar después

**Tareas:**
- [ ] Implementar `test_k8s_full_job_execution()`
- [ ] Verificar logs via LogStreamService
- [ ] Verificar estado final en Postgres

### HU-10.5: Test E2E de Cleanup de Pods

**Como** desarrollador  
**Quiero** un test que verifique la eliminación de Pods  
**Para** evitar Pods huérfanos

**Criterios de Aceptación:**
- [ ] Pod se elimina después de destroy_worker
- [ ] No quedan recursos huérfanos
- [ ] Cleanup funciona incluso si Pod está en error

**Tareas:**
- [ ] Implementar `test_k8s_pod_cleanup()`
- [ ] Verificar que Pod no existe después de cleanup
- [ ] Probar cleanup de Pod en estado CrashLoopBackOff

### HU-10.6: Test E2E de Múltiples Workers

**Como** desarrollador  
**Quiero** un test que verifique el scaling de workers  
**Para** validar el manejo de múltiples Pods

**Criterios de Aceptación:**
- [ ] Se pueden crear múltiples Pods workers
- [ ] Cada Pod tiene su propio OTP
- [ ] Jobs se distribuyen entre workers

**Tareas:**
- [ ] Implementar `test_k8s_multiple_workers()`
- [ ] Verificar que cada Pod se registra correctamente
- [ ] Verificar distribución de jobs

### HU-10.7: Script de Ejecución E2E Kubernetes

**Como** desarrollador  
**Quiero** un script que ejecute todos los tests E2E de Kubernetes  
**Para** automatizar la validación

**Criterios de Aceptación:**
- [ ] Script verifica/crea cluster Kind
- [ ] Carga imagen worker en cluster
- [ ] Ejecuta tests E2E
- [ ] Limpia recursos al finalizar

**Tareas:**
- [ ] Crear `scripts/e2e/run-kubernetes-e2e.sh`
- [ ] Integrar con CI/CD
- [ ] Documentar en GETTING_STARTED.md

## Requisitos Técnicos

### Dependencias

```toml
# Cargo.toml (dev-dependencies)
[dev-dependencies]
kube = { version = "0.87", features = ["runtime", "derive"] }
k8s-openapi = { version = "0.20", features = ["v1_28"] }
```

### Variables de Entorno para Tests

| Variable | Valor | Descripción |
|----------|-------|-------------|
| `HODEI_E2E_K8S` | `1` | Habilita tests E2E de K8s |
| `HODEI_K8S_TEST_NAMESPACE` | `hodei-e2e` | Namespace para tests |
| `HODEI_WORKER_IMAGE` | `hodei-worker:e2e-test` | Imagen del worker |
| `KIND_CLUSTER_NAME` | `hodei-e2e` | Nombre del cluster Kind |

### Estructura de Archivos

```
crates/grpc/tests/
├── e2e_common.rs              # Helpers compartidos (de EPIC-9)
├── e2e_docker_provider.rs     # Tests E2E Docker
├── e2e_kubernetes_provider.rs # Tests E2E Kubernetes ← NUEVO

scripts/e2e/
├── run-kubernetes-e2e.sh      # Script principal
├── setup-kind-cluster.sh      # Setup cluster
└── cleanup-kind.sh            # Limpiar cluster

deploy/kubernetes/
├── rbac-test.yaml             # RBAC para tests
└── namespace-test.yaml        # Namespace para tests
```

## Desafíos Específicos de Kubernetes

### Networking Host ↔ Cluster

**Problema:** El server corre en el host, los Pods en el cluster.

**Soluciones:**
1. **Kind con Docker:** Usar `host.docker.internal`
2. **Port-forward:** `kubectl port-forward` desde Pod al host
3. **NodePort:** Exponer server via NodePort en el cluster

### Tiempo de Startup de Pods

**Problema:** Los Pods tardan más en iniciar que containers Docker.

**Solución:** Timeouts más generosos (30-60s vs 10s para Docker).

### Image Pull

**Problema:** La imagen debe estar disponible en el cluster.

**Solución:** `kind load docker-image` para cargar imagen local.

## Estimación

| Historia | Complejidad | Estimación |
|----------|-------------|------------|
| HU-10.1 | Alta | 4h |
| HU-10.2 | Alta | 3h |
| HU-10.3 | Media | 3h |
| HU-10.4 | Alta | 4h |
| HU-10.5 | Media | 2h |
| HU-10.6 | Media | 3h |
| HU-10.7 | Baja | 2h |
| **Total** | | **21h** |

## Dependencias

- **EPIC-9**: Reutiliza `e2e_common.rs` y patrones de test
- **EPIC-7**: KubernetesProvider debe estar completado ✅
- **Kind**: Debe estar instalado en el sistema

## Criterios de Éxito

1. Tests pasan con cluster Kind local
2. Tests pasan con cluster real (opcional)
3. Cleanup completo sin Pods huérfanos
4. Documentación clara de setup
