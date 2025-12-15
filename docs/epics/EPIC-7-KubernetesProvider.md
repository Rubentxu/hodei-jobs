# EPIC-7: Kubernetes Provider

## Resumen

Implementar un provider de Kubernetes que permita a Hodei provisionar workers como Pods en clústeres Kubernetes, utilizando la librería `kube-rs` para interacción nativa con la API de Kubernetes.

---

## Objetivos

1. Provisionar workers como Pods en Kubernetes
2. Soportar múltiples clústeres (EKS, GKE, AKS, vanilla)
3. Integrar con RBAC y Network Policies de Kubernetes
4. Mantener compatibilidad con el trait `WorkerProvider` existente

---

## Dependencias

- EPIC-6 completado (Control Loop funcional)
- Acceso a clúster Kubernetes para testing
- Documentación técnica: `docs/providers/kubernetes-provider-design.md`

---

## Historias de Usuario

### HU-7.1: Configuración del Kubernetes Provider

**Como** operador de plataforma  
**quiero** configurar el Kubernetes Provider con credenciales y namespace  
**para** que Hodei pueda crear Pods en mi clúster

**Criterios de aceptación:**

- [ ] Crear `KubernetesConfig` struct con campos:
  - `namespace` (default: "hodei-workers")
  - `kubeconfig_path` (opcional, None = in-cluster)
  - `context` (opcional)
  - `service_account`
  - `node_selector`
  - `tolerations`
  - `image_pull_secrets`
- [ ] Implementar `KubernetesConfigBuilder` con Builder Pattern
- [ ] Cargar configuración desde variables de entorno
- [ ] Validar configuración al inicializar provider
- [ ] Tests unitarios para configuración

**Estimación:** 3 puntos

#### Checks de validación (HU-7.1)

- [x] `KubernetesConfig` struct con todos los campos requeridos
- [x] `KubernetesConfigBuilder` con Builder Pattern
- [x] `KubernetesToleration` con Builder Pattern
- [x] Carga de configuración desde variables de entorno (`from_env()`)
- [x] Validación de configuración (namespace no vacío)
- [x] Tests unitarios: `test_kubernetes_config_default`, `test_kubernetes_config_builder`, `test_kubernetes_config_builder_validation`, `test_toleration_builder`

---

### HU-7.2: Conexión al Clúster Kubernetes

**Como** sistema  
**quiero** conectar al clúster Kubernetes usando kube-rs  
**para** poder interactuar con la API de Kubernetes

**Criterios de aceptación:**

- [ ] Crear `KubernetesProvider` struct implementando `WorkerProvider`
- [ ] Soportar conexión in-cluster (ServiceAccount)
- [ ] Soportar conexión via kubeconfig
- [ ] Implementar `health_check()` que verifica conectividad al API server
- [ ] Manejar errores de conexión con reintentos exponenciales
- [ ] Tests unitarios con mock de kube::Client

**Dependencias técnicas:**
```toml
kube = { version = "0.98", features = ["runtime", "client"] }
k8s-openapi = { version = "0.24", features = ["latest"] }
```

**Estimación:** 5 puntos

#### Checks de validación (HU-7.2)

- [x] `KubernetesProvider` struct implementando `WorkerProvider`
- [x] Soporte conexión in-cluster (`Config::infer()`)
- [x] Soporte conexión via kubeconfig (`Config::from_custom_kubeconfig()`)
- [x] `health_check()` verifica conectividad al API server
- [x] Manejo de errores de conexión con mensajes descriptivos
- [x] Tests unitarios con verificación de capabilities

---

### HU-7.3: Creación de Worker Pods

**Como** scheduler  
**quiero** crear Pods de worker en Kubernetes  
**para** ejecutar jobs en el clúster

**Criterios de aceptación:**

- [ ] Implementar `create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle>`
- [ ] Generar Pod spec con:
  - Nombre: `hodei-worker-{worker_id}`
  - Labels: `app=hodei-worker`, `hodei.io/worker-id`, `hodei.io/managed=true`
  - Container con imagen del worker
  - Variables de entorno: `HODEI_WORKER_ID`, `HODEI_SERVER_ADDRESS`, `HODEI_OTP_TOKEN`
  - Resource requests/limits desde `WorkerSpec`
- [ ] Aplicar `nodeSelector` y `tolerations` de configuración
- [ ] Manejar `imagePullSecrets`
- [ ] Retornar `WorkerHandle` con `provider_resource_id` = Pod name
- [ ] Tests de integración con kind/minikube

**Estimación:** 8 puntos

#### Checks de validación (HU-7.3)

- [x] `create_worker()` implementado con Pod spec completo
- [x] Pod name: `hodei-worker-{worker_id}`
- [x] Labels: `app=hodei-worker`, `hodei.io/worker-id`, `hodei.io/managed=true`
- [x] Variables de entorno: `HODEI_WORKER_ID`, `HODEI_SERVER_ADDRESS`, `HODEI_OTP_TOKEN`
- [x] Resource requests/limits desde `WorkerSpec`
- [x] `nodeSelector` y `tolerations` aplicados
- [x] `imagePullSecrets` configurados
- [x] `WorkerHandle` retornado con `provider_resource_id` = Pod name
- [x] Test: `test_pod_name_generation`

---

### HU-7.4: Monitoreo de Estado de Pods

**Como** sistema  
**quiero** monitorear el estado de los Pods de worker  
**para** actualizar el estado en el WorkerRegistry

**Criterios de aceptación:**

- [ ] Implementar `get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState>`
- [ ] Mapear Pod phases a WorkerState:
  - Pending → Creating
  - Running → Ready (si container ready) / Busy (si ejecutando job)
  - Succeeded → Terminated
  - Failed → Terminated
  - Unknown → Creating
- [ ] Detectar container restarts y reportar
- [ ] Manejar Pod not found (worker terminado externamente)
- [ ] Tests unitarios para mapeo de estados

**Estimación:** 3 puntos

#### Checks de validación (HU-7.4)

- [x] `get_worker_status()` implementado
- [x] Mapeo de Pod phases a WorkerState:
  - Pending → Creating
  - Running (ready) → Ready
  - Running (not ready) → Connecting
  - Succeeded → Terminated
  - Failed → Terminated
- [x] Detección de container ready status
- [x] Manejo de Pod not found (retorna Terminated)
- [x] Test: `test_map_pod_phase`

---

### HU-7.5: Destrucción de Worker Pods

**Como** sistema  
**quiero** destruir Pods de worker cuando ya no son necesarios  
**para** liberar recursos del clúster

**Criterios de aceptación:**

- [ ] Implementar `destroy_worker(&self, handle: &WorkerHandle) -> Result<()>`
- [ ] Enviar delete con grace period configurable
- [ ] Esperar confirmación de eliminación
- [ ] Manejar Pod already deleted (idempotente)
- [ ] Limpiar recursos asociados (PVCs si aplica)
- [ ] Tests de integración

**Estimación:** 3 puntos

#### Checks de validación (HU-7.5)

- [x] `destroy_worker()` implementado
- [x] Delete con grace period (30s)
- [x] Manejo de Pod already deleted (idempotente, retorna Ok)
- [x] Logging de operaciones

---

### HU-7.6: Obtención de Logs de Pods

**Como** operador  
**quiero** obtener logs de los Pods de worker  
**para** diagnosticar problemas

**Criterios de aceptación:**

- [ ] Implementar `get_worker_logs(&self, handle: &WorkerHandle, tail: Option<u32>) -> Result<Vec<LogEntry>>`
- [ ] Obtener logs del container principal
- [ ] Soportar `tail` para limitar líneas
- [ ] Parsear timestamps de logs de Kubernetes
- [ ] Manejar Pod sin logs (recién creado)
- [ ] Tests unitarios

**Estimación:** 3 puntos

#### Checks de validación (HU-7.6)

- [x] `get_worker_logs()` implementado
- [x] Soporte `tail` para limitar líneas
- [x] Timestamps habilitados en logs
- [x] Parseo de timestamps de K8s
- [x] Tests: `test_parse_k8s_log_line`, `test_parse_k8s_log_line_no_timestamp`

---

### HU-7.7: Capacidades del Provider

**Como** scheduler  
**quiero** conocer las capacidades del Kubernetes Provider  
**para** tomar decisiones de scheduling informadas

**Criterios de aceptación:**

- [ ] Implementar `capabilities()` retornando `ProviderCapabilities`:
  - `max_resources`: basado en límites del namespace o configuración
  - `gpu_support`: detectar si hay nodos con GPU
  - `architectures`: detectar arquitecturas disponibles (amd64, arm64)
  - `regions`: mapear a zonas de disponibilidad
- [ ] Implementar `can_fulfill(&self, requirements: &JobRequirements) -> bool`
- [ ] Implementar `estimated_startup_time()` (típicamente 5-30s)
- [ ] Tests unitarios

**Estimación:** 5 puntos

#### Checks de validación (HU-7.7)

- [x] `capabilities()` retorna `ProviderCapabilities` completo
- [x] `max_resources` configurado (64 CPU, 256GB RAM)
- [x] `gpu_support: true`
- [x] `architectures: [Amd64, Arm64]`
- [x] `estimated_startup_time()` retorna 15s
- [x] Test: `test_default_capabilities`

---

### HU-7.8: Wiring en Server

**Como** operador  
**quiero** habilitar el Kubernetes Provider en el servidor  
**para** usar Kubernetes para provisioning

**Criterios de aceptación:**

- [ ] Añadir `KubernetesProvider` a `server.rs`
- [ ] Configurar via variables de entorno:
  - `HODEI_K8S_ENABLED=1`
  - `HODEI_K8S_NAMESPACE`
  - `HODEI_K8S_KUBECONFIG`
- [ ] Registrar provider en `DefaultWorkerProvisioningService`
- [ ] Log de inicialización indicando estado del provider
- [ ] Documentar configuración en README

**Estimación:** 3 puntos

#### Checks de validación (HU-7.8)

- [x] `KubernetesProvider` importado en `server.rs`
- [x] Configuración via variables de entorno:
  - `HODEI_K8S_ENABLED=1` para habilitar
  - `HODEI_K8S_NAMESPACE` para namespace
  - `HODEI_K8S_KUBECONFIG` para kubeconfig path
- [x] Provider registrado en `DefaultWorkerProvisioningService`
- [x] Logging de inicialización del provider
- [x] Compilación sin errores ni warnings

---

### HU-7.9: RBAC y Manifiestos de Kubernetes

**Como** operador  
**quiero** manifiestos de Kubernetes para desplegar Hodei  
**para** configurar permisos y recursos correctamente

**Criterios de aceptación:**

- [ ] Crear `deploy/kubernetes/` con:
  - `namespace.yaml` - Namespace hodei-workers
  - `rbac.yaml` - Role, RoleBinding, ServiceAccount
  - `network-policy.yaml` - Políticas de red
  - `configmap.yaml` - Configuración del provider
- [ ] Documentar requisitos de RBAC mínimos
- [ ] Script de instalación `deploy/kubernetes/install.sh`
- [ ] Tests de validación de manifiestos

**Estimación:** 5 puntos

#### Checks de validación (HU-7.9)

- [x] `deploy/kubernetes/` creado con:
  - `namespace.yaml` - Namespace hodei-workers
  - `rbac.yaml` - Role, RoleBinding, ServiceAccounts
  - `network-policy.yaml` - Políticas de red (egress/ingress)
  - `configmap.yaml` - Configuración del provider
  - `README.md` - Documentación de despliegue
- [x] Script de instalación `deploy/kubernetes/install.sh`
- [x] RBAC mínimo documentado (pods: create, get, list, watch, delete; pods/log: get)
- [x] Network policies configuradas (DNS, gRPC, HTTPS)

---

### HU-7.10: Tests de Integración con Kind

**Como** desarrollador  
**quiero** tests de integración automatizados  
**para** validar el Kubernetes Provider en CI

**Criterios de aceptación:**

- [ ] Configurar kind cluster en CI
- [ ] Tests de integración:
  - Crear worker Pod
  - Verificar estado
  - Obtener logs
  - Destruir Pod
- [ ] Tests marcados con `#[ignore]` para ejecución local sin K8s
- [ ] Documentar cómo ejecutar tests localmente

**Estimación:** 5 puntos

#### Checks de validación (HU-7.10)

- [x] Tests de integración creados en `crates/infrastructure/tests/kubernetes_integration.rs`
- [x] Tests marcados con `#[ignore]` para ejecución sin K8s
- [x] Variable de entorno `HODEI_K8S_TEST=1` para habilitar tests
- [x] Tests cubren:
  - Health check del provider
  - Crear y destruir worker Pod
  - Destroy idempotente para pods no existentes
  - Verificación de capabilities
  - Rechazo de workers duplicados
- [x] Documentación de cómo ejecutar tests localmente

---

## Criterios de Aceptación de la Épica

- [x] Todos los tests unitarios pasan (9 tests en kubernetes.rs)
- [x] Tests de integración creados (5 tests con `#[ignore]`)
- [x] Documentación actualizada (design doc, README, EPIC)
- [x] No warnings de compilación
- [x] Provider funcional con trait `WorkerProvider`
- [ ] Métricas Prometheus exportadas (pendiente)

### Resumen de Implementación

**Archivos creados:**
- `crates/infrastructure/src/providers/kubernetes.rs` - Provider completo (~900 líneas)
- `crates/infrastructure/tests/kubernetes_integration.rs` - Tests de integración
- `deploy/kubernetes/namespace.yaml` - Namespace manifest
- `deploy/kubernetes/rbac.yaml` - RBAC manifest
- `deploy/kubernetes/network-policy.yaml` - Network policies
- `deploy/kubernetes/configmap.yaml` - ConfigMap
- `deploy/kubernetes/install.sh` - Script de instalación
- `deploy/kubernetes/README.md` - Documentación de despliegue

**Archivos modificados:**
- `Cargo.toml` - Añadidas dependencias kube-rs, k8s-openapi
- `crates/infrastructure/Cargo.toml` - Dependencias del crate
- `crates/infrastructure/src/providers/mod.rs` - Export del módulo
- `crates/grpc/src/bin/server.rs` - Wiring del provider

**Tests unitarios:**
```
cargo test -p hodei-jobs-infrastructure providers::kubernetes
```
- `test_kubernetes_config_default`
- `test_kubernetes_config_builder`
- `test_kubernetes_config_builder_validation`
- `test_toleration_builder`
- `test_pod_name_generation`
- `test_map_pod_phase`
- `test_parse_k8s_log_line`
- `test_parse_k8s_log_line_no_timestamp`
- `test_default_capabilities`

**Variables de entorno:**
- `HODEI_K8S_ENABLED=1` - Habilitar provider
- `HODEI_K8S_NAMESPACE` - Namespace para workers
- `HODEI_K8S_KUBECONFIG` - Path a kubeconfig
- `HODEI_K8S_CONTEXT` - Contexto de kubeconfig
- `HODEI_K8S_SERVICE_ACCOUNT` - ServiceAccount para pods

---

## Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Incompatibilidad de versiones K8s | Media | Alto | Testear con múltiples versiones |
| Permisos RBAC insuficientes | Alta | Medio | Documentar requisitos claramente |
| Latencia de API de K8s | Baja | Medio | Implementar caching y timeouts |

---

## Estimación Total

| Historia | Puntos |
|----------|--------|
| HU-7.1 | 3 |
| HU-7.2 | 5 |
| HU-7.3 | 8 |
| HU-7.4 | 3 |
| HU-7.5 | 3 |
| HU-7.6 | 3 |
| HU-7.7 | 5 |
| HU-7.8 | 3 |
| HU-7.9 | 5 |
| HU-7.10 | 5 |
| **Total** | **43** |

---

## Referencias

- [Diseño Técnico](../providers/kubernetes-provider-design.md)
- [kube-rs Documentation](https://kube.rs/)
- [PRD-V7.0](../PRD-V7.0.md) - Sección 5.2 Modelo de Proveedores
