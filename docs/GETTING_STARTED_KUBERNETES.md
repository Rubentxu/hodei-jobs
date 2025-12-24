# Hodei Job Platform - Guía de Inicio: Kubernetes Provider

**Versión**: 8.0  
**Última Actualización**: 2025-12-18

Esta guía te ayudará a configurar y utilizar el **Kubernetes Provider** en Hodei Job Platform. Este provider permite aprovisionar Workers dinámicamente como Pods dentro de un clúster de Kubernetes.

## 🚀 Worker Agent v8.0 - HPC Ready en Kubernetes

El worker agent v8.0 incluye optimizaciones específicas para entornos Kubernetes:

### Beneficios en Kubernetes

| Optimización | Beneficio en K8s |
|--------------|------------------|
| **LogBatching** | 90-99% reducción en calls gRPC desde el Pod |
| **CGroups Integration** | Métricas precisas desde cgroups del contenedor |
| **Cached Metrics** | Menor overhead en recolección de métricas |
| **Zero-Copy I/O** | Menor uso de memoria en el Pod |
| **Async Cleanup** | Limpieza no bloqueante de recursos |
| **mTLS Ready** | Zero Trust security con certificados |

### Consideraciones de Rendimiento

**LogBatching en Kubernetes**:
- Los logs se agrupan en el Pod antes de enviarse
- Reduce la sobrecarga de red del clúster
- Mejor throughput para jobs con muchos logs

**Métricas con CGroups**:
- Lectura directa de cgroups del contenedor
- Métricas más precisas que `kubectl top`
- Cache TTL reduce la carga en cgroup filesystem

## 📋 Prerrequisitos

Para utilizar este provider, necesitas:

1.  **Clúster de Kubernetes** accesible (Local o Remoto).
    - Recomendado para desarrollo: [Kind](https://kind.sigs.k8s.io/) o [Minikube](https://minikube.sigs.k8s.io/).
    - También compatible con EKS, GKE, AKS, etc.
2.  **`kubectl`** instalado y configurado apuntando a tu clúster.
3.  **Permisos**: El contexto de Kubernetes debe tener permisos para crear `Namespaces`, `Pods`, `Services`, y `ServiceAccounts`.

## 🐳 Construcción de la Imagen Worker

El provider de Kubernetes necesita que la imagen del worker (`hodei-jobs-worker`) contenga el binario del agente para poder comunicarse con el servidor. Debes construir esta imagen localmente.

1. **Construir la imagen:**

Desde la raíz del proyecto:

```bash
docker build -f Dockerfile.worker -t hodei-jobs-worker:latest .
```

2. **Cargar la imagen en el clúster (Solo Desarrollo Local):**

Si usas **Kind** o **Minikube**, el clúster no verá la imagen local a menos que la cargues explícitamente:

- **Kind**:

  ```bash
  kind load docker-image hodei-jobs-worker:latest
  ```

- **Minikube**:
  ```bash
  minikube image load hodei-jobs-worker:latest
  ```

### Configuración Básica (Tests E2E)

Para que los tests de integración funcionen (y para desarrollo local), el código espera encontrar un `ServiceAccount` llamado `hodei-jobs-worker` en el namespace `hodei-jobs-workers`.
El provider asigna esta cuenta a los Pods creados.

1. Crea el archivo `k8s/rbac.yaml` (o usa el existente) para crear este recurso:

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: hodei-jobs-worker
  namespace: hodei-jobs-workers
---
# Opcional: Rol para que el worker pueda inspeccionarse a sí mismo (si fuera necesario)
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: hodei-jobs-worker-role
  namespace: hodei-jobs-workers
rules:
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: hodei-jobs-worker-binding
  namespace: hodei-jobs-workers
subjects:
  - kind: ServiceAccount
    name: hodei-jobs-worker
    namespace: hodei-jobs-workers
roleRef:
  kind: Role
  name: hodei-jobs-worker-role
  apiGroup: rbac.authorization.k8s.io
```

2. Aplica la configuración:

```bash
kubectl apply -f k8s/rbac.yaml
```

### Despliegue de Producción

El directorio `deploy/kubernetes` contiene los manifiestos completos para un despliegue en clúster, incluyendo:

- `install.sh`: Script de instalación automatizada.
- `rbac.yaml`: Configura permisos para el Control Plane (`hodei-system`) sobre los Workers.
- `network-policy.yaml`: Aislamiento de red para los workers.

Estos manifiestos han sido actualizados para coincidir con los valores por defecto del código:

- Namespace: `hodei-jobs-workers`
- Service Account: `hodei-jobs-worker`

## ⚙️ Configuración del Servidor

Asegúrate de que tu entorno (donde corre `hodei-jobs-server`) tenga las siguientes variables configuradas o que el archivo `kubeconfig` esté en la ubicación estándar (`~/.kube/config`).

```bash
# Opcional, por defecto usa ~/.kube/config
export KUBECONFIG=/path/to/your/kubeconfig

# Namespace donde se crearán los workers (Default: hodei-jobs-workers)
export HODEI_K8S_NAMESPACE=hodei-jobs-workers
```

### Registrar el Provider

Si estás corriendo el servidor localmente, debes registrar el provider de Kubernetes.

```bash
# Ejemplo usando cURL o la CLI (si está disponible)
curl -X POST http://localhost:8080/api/providers \
  -H "Content-Type: application/json" \
  -d '{
    "type": "kubernetes",
    "name": "k8s-local",
    "config": {
      "namespace": "hodei-jobs-workers",
      "image_pull_policy": "IfNotPresent"
    }
  }'
```

## 🚀 Verificación y Testing

Hodei Job Platform incluye una suite de tests E2E diseñada específicamente para validar la integración con Kubernetes.

### Validar flujo E2E (Development)

Puedes ejecutar los tests de integración que verifican desde la conexión con el clúster hasta el ciclo de vida de los Pods. Estos tests asumen la **Opción A (Desarrollo Manual)** por defecto.

**Comando de verificación:**

```bash
# Habilita los tests de K8s y muestra la salida
HODEI_K8S_TEST=1 cargo test --test e2e_kubernetes_provider -- --ignored --nocapture
```

### Niveles de Test

La suite valida 4 niveles de integración:

1.  **Infraestructura Base**:

    - Verifica acceso a `kubectl`.
    - Verifica conexión al clúster.
    - Crea el namespace de test si no existe.

2.  **Registro de Workers**:

    - Simula el registro de un worker (sin pod real) para probar la API.

3.  **Ciclo de Vida del Provider**:

    - **Creación**: El provider solicita crear un Pod.
    - **Estado**: Verifica que el Pod pasa a `Running`.
    - **Logs**: Intenta obtener logs del contenedor.
    - **Destrucción**: Elimina el Pod y verifica su terminación.

4.  **Flujo Completo** (Opcional):
    - Requiere la imagen `hodei-jobs-worker` disponible en el clúster.

## 🛠️ Solución de Problemas

### Error: `kubectl not available`

Asegúrate de que el binario `kubectl` está en el PATH de la máquina donde corren los tests o el servidor.

### Error: `Kubernetes cluster not available`

Verifica tu conexión:

```bash
kubectl cluster-info
```

### Error: `serviceaccount "hodei-jobs-worker" not found`

Asegúrate de haber aplicado el archivo `k8s/rbac.yaml` (Opción A) o de haber configurado `HODEI_K8S_SERVICE_ACCOUNT` si usas la Opción B.

### Pods en estado `ImagePullBackOff`

Si usas **Kind** o **Minikube**, es posible que el clúster no pueda ver tu imagen Docker local.

- **Kind**: `kind load docker-image hodei-jobs-worker:latest`
- **Minikube**: `minikube image load hodei-jobs-worker:latest`

O configura `image_pull_policy: "Always"` y usa una imagen de un registro público.

## 🔐 Configuración de mTLS en Kubernetes (v8.0)

Para habilitar Zero Trust security con mTLS:

### 1. Generar Certificados

```bash
# Desde el directorio del proyecto
./scripts/Worker\ Management/generate-certificates.sh
```

Esto creará:
- `certs/ca.crt` - CA root certificate
- `certs/server.crt` - Server certificate
- `certs/server.key` - Server private key
- `certs/client.crt` - Client certificate (para workers)
- `certs/client.key` - Client private key

### 2. Crear Secrets en Kubernetes

```bash
# Crear secret para CA certificate
kubectl create secret generic hodei-ca-cert \
  --from-file=ca.crt=certs/ca.crt \
  --namespace=hodei-system

# Crear secret para client certificate (workers)
kubectl create secret generic hodei-client-cert \
  --from-file=client.crt=certs/client.crt \
  --from-file=client.key=certs/client.key \
  --namespace=hodei-jobs-workers
```

### 3. Configurar Variables de Entorno

En el Deployment del worker:

```yaml
env:
  - name: HODEI_MTLS_ENABLED
    value: "1"
  - name: HODEI_CLIENT_CERT_PATH
    value: "/etc/hodei/certs/client.crt"
  - name: HODEI_CLIENT_KEY_PATH
    value: "/etc/hodei/certs/client.key"
  - name: HODEI_CA_CERT_PATH
    value: "/etc/hodei/certs/ca.crt"
```

### 4. Montar Certificados

```yaml
volumeMounts:
  - name: client-cert
    mountPath: "/etc/hodei/certs"
    readOnly: true
  - name: ca-cert
    mountPath: "/etc/hodei/ca"
    readOnly: true

volumes:
  - name: client-cert
    secret:
      secretName: hodei-client-cert
  - name: ca-cert
    secret:
      secretName: hodei-ca-cert
      items:
        - key: ca.crt
          path: ca.crt
```

**Nota**: Requiere upgrade a `tonic >= 0.15` para habilitar TLS features.

## 📊 Monitoreo de Performance en Kubernetes

### Verificar LogBatching

```bash
# Ver logs del worker
kubectl logs -f deployment/hodei-worker -n hodei-jobs-workers

# Buscar indicadores de batching
grep -i "batch\|flush" /var/log/hodei/worker.log
```

### Verificar Métricas de CGroups

```bash
# Ejecutar en el worker pod
kubectl exec -it <worker-pod> -n hodei-jobs-workers -- bash

# Ver cgroup stats
cat /sys/fs/cgroup/memory/memory.usage_in_bytes
cat /sys/fs/cgroup/cpu/cpuacct.usage
```

### Métricas de Performance Esperadas

| Métrica | Valor Esperado |
|---------|----------------|
| gRPC calls reduction | 90-99% |
| Memory allocation reduction | ~40% |
| Metrics collection overhead | ~60% reduction |
| Log throughput | > 10k logs/sec |
