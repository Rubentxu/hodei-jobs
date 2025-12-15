# Kubernetes Provider - Diseño Técnico

## Resumen Ejecutivo

El **Kubernetes Provider** permite a Hodei provisionar workers como Pods en clústeres Kubernetes (vanilla, EKS, GKE, AKS). Utiliza la librería `kube-rs` para interactuar con la API de Kubernetes de forma nativa en Rust.

---

## 1. Arquitectura

### 1.1 Componentes

```
┌─────────────────────────────────────────────────────────────┐
│                    Hodei Control Plane                       │
│  ┌─────────────────┐    ┌─────────────────┐                 │
│  │ SchedulerService│───▶│ KubernetesProvider│               │
│  └─────────────────┘    └────────┬────────┘                 │
│                                  │                           │
└──────────────────────────────────┼───────────────────────────┘
                                   │ kube-rs API
                                   ▼
┌─────────────────────────────────────────────────────────────┐
│                    Kubernetes Cluster                        │
│  ┌─────────────────┐    ┌─────────────────┐                 │
│  │   Worker Pod    │    │   Worker Pod    │                 │
│  │ ┌─────────────┐ │    │ ┌─────────────┐ │                 │
│  │ │ Hodei Agent │ │    │ │ Hodei Agent │ │                 │
│  │ └─────────────┘ │    │ └─────────────┘ │                 │
│  └─────────────────┘    └─────────────────┘                 │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 Flujo de Provisioning

1. **Scheduler** decide `ProvisionWorker` con `provider_id` de Kubernetes
2. **KubernetesProvider** crea un Pod con la imagen del worker
3. Pod incluye variables de entorno: `HODEI_WORKER_ID`, `HODEI_SERVER_ADDRESS`, `HODEI_OTP_TOKEN`
4. Agent en el Pod conecta al Control Plane via gRPC
5. Worker se registra y queda disponible para jobs

---

## 2. Dependencias

### 2.1 Crates Rust

```toml
[dependencies]
kube = { version = "0.98", features = ["runtime", "client", "derive"] }
k8s-openapi = { version = "0.24", features = ["latest"] }
tokio = { version = "1", features = ["full"] }
```

### 2.2 Versiones de Kubernetes Soportadas

| Kubernetes Version | k8s-openapi feature |
|--------------------|---------------------|
| 1.31.x             | `v1_31`             |
| 1.30.x             | `v1_30`             |
| 1.29.x             | `v1_29`             |
| 1.28.x             | `v1_28`             |

---

## 3. Modelo de Datos

### 3.1 KubernetesConfig

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfig {
    /// Namespace donde crear los Pods (default: "hodei-workers")
    pub namespace: String,
    /// Kubeconfig path (None = in-cluster config)
    pub kubeconfig_path: Option<String>,
    /// Context del kubeconfig (None = current-context)
    pub context: Option<String>,
    /// Service Account para los Pods
    pub service_account: Option<String>,
    /// Labels base para todos los Pods
    pub base_labels: HashMap<String, String>,
    /// Annotations base para todos los Pods
    pub base_annotations: HashMap<String, String>,
    /// Node selector para scheduling
    pub node_selector: HashMap<String, String>,
    /// Tolerations para los Pods
    pub tolerations: Vec<Toleration>,
    /// Resource defaults
    pub default_resources: ResourceRequirements,
    /// Image pull secrets
    pub image_pull_secrets: Vec<String>,
    /// TTL seconds after finished (cleanup)
    pub ttl_seconds_after_finished: Option<i32>,
}
```

### 3.2 Toleration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toleration {
    pub key: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub effect: Option<String>,
    pub toleration_seconds: Option<i64>,
}
```

---

## 4. Implementación del Trait WorkerProvider

```rust
#[async_trait]
impl WorkerProvider for KubernetesProvider {
    fn provider_id(&self) -> &ProviderId;
    fn provider_type(&self) -> ProviderType;
    fn capabilities(&self) -> &ProviderCapabilities;
    
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError>;
    async fn get_worker_logs(&self, handle: &WorkerHandle, tail: Option<u32>) -> Result<Vec<LogEntry>, ProviderError>;
    async fn health_check(&self) -> Result<HealthStatus, ProviderError>;
}
```

---

## 5. Mapeo de Estados

| Pod Phase     | WorkerState   |
|---------------|---------------|
| Pending       | Creating      |
| Running       | Ready/Busy    |
| Succeeded     | Terminated    |
| Failed        | Terminated    |
| Unknown       | Creating      |

---

## 6. Pod Spec Template

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: hodei-worker-{worker_id}
  namespace: hodei-workers
  labels:
    app: hodei-worker
    hodei.io/worker-id: "{worker_id}"
    hodei.io/provider-id: "{provider_id}"
    hodei.io/managed: "true"
spec:
  restartPolicy: Never
  serviceAccountName: hodei-worker
  containers:
  - name: worker
    image: "{worker_image}"
    env:
    - name: HODEI_WORKER_ID
      value: "{worker_id}"
    - name: HODEI_SERVER_ADDRESS
      value: "{server_address}"
    - name: HODEI_OTP_TOKEN
      value: "{otp_token}"
    resources:
      requests:
        cpu: "{cpu_request}"
        memory: "{memory_request}"
      limits:
        cpu: "{cpu_limit}"
        memory: "{memory_limit}"
  nodeSelector: {}
  tolerations: []
```

---

## 7. Seguridad

### 7.1 RBAC Requerido

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: hodei-provider
  namespace: hodei-workers
rules:
- apiGroups: [""]
  resources: ["pods"]
  verbs: ["create", "get", "list", "watch", "delete"]
- apiGroups: [""]
  resources: ["pods/log"]
  verbs: ["get"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: hodei-provider
  namespace: hodei-workers
subjects:
- kind: ServiceAccount
  name: hodei-control-plane
  namespace: hodei-system
roleRef:
  kind: Role
  name: hodei-provider
  apiGroup: rbac.authorization.k8s.io
```

### 7.2 Network Policies

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: hodei-workers-egress
  namespace: hodei-workers
spec:
  podSelector:
    matchLabels:
      app: hodei-worker
  policyTypes:
  - Egress
  egress:
  - to:
    - namespaceSelector:
        matchLabels:
          name: hodei-system
    ports:
    - protocol: TCP
      port: 50051  # gRPC
```

---

## 8. Configuración de Entorno

### Variables de Entorno del Provider

| Variable | Descripción | Default |
|----------|-------------|---------|
| `HODEI_K8S_NAMESPACE` | Namespace para workers | `hodei-workers` |
| `HODEI_K8S_KUBECONFIG` | Path al kubeconfig | In-cluster |
| `HODEI_K8S_CONTEXT` | Contexto del kubeconfig | Current |
| `HODEI_K8S_SERVICE_ACCOUNT` | Service Account | `hodei-worker` |
| `HODEI_K8S_IMAGE_PULL_SECRET` | Secret para pull de imágenes | - |

---

## 9. Métricas y Observabilidad

### Métricas Prometheus

- `hodei_k8s_pods_created_total` - Total de Pods creados
- `hodei_k8s_pods_failed_total` - Total de Pods fallidos
- `hodei_k8s_pod_startup_duration_seconds` - Tiempo de startup de Pods
- `hodei_k8s_api_requests_total` - Requests a la API de K8s
- `hodei_k8s_api_errors_total` - Errores de API

---

## 10. Testing

### 10.1 Unit Tests

- Mock de `kube::Client` para tests sin cluster
- Validación de Pod spec generation
- Mapeo de estados

### 10.2 Integration Tests

- Requiere cluster Kubernetes (kind/minikube)
- Tests con `#[ignore]` para CI sin K8s
- Testcontainers con kind

---

## 11. Limitaciones Conocidas

1. **No soporta Jobs de Kubernetes** - Usa Pods directamente para control fino
2. **No soporta StatefulSets** - Workers son efímeros
3. **Requiere RBAC configurado** - No auto-configura permisos
4. **Single namespace** - Un provider = un namespace

---

## 12. Referencias

- [kube-rs Documentation](https://kube.rs/)
- [kube-rs GitHub](https://github.com/kube-rs/kube)
- [k8s-openapi](https://docs.rs/k8s-openapi/)
- [Kubernetes API Reference](https://kubernetes.io/docs/reference/kubernetes-api/)
