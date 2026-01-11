# Flujo de Pruebas Completo: Minikube + Operator + DevSpace + Skill

Este documento describe el flujo completo para probar el sistema Hodei Jobs Platform usando Minikube, DevSpace y el skill pruebas-cli.

## 📋 Tabla de Contenidos

1. [Fase 1: Preparación del Entorno](#fase-1-preparación-del-entorno)
2. [Fase 2: Despliegue de Infraestructura](#fase-2-despliegue-de-infraestructura)
3. [Fase 3: Despliegue del Operator](#fase-3-despliegue-del-operator)
4. [Fase 4: Desarrollo con DevSpace](#fase-4-desarrollo-con-devspace)
5. [Fase 5: Pruebas End-to-End](#fase-5-pruebas-end-to-end)
6. [Fase 6: Verificación y Debugging](#fase-6-verificación-y-debugging)

---

## 🎯 Resumen del Flujo

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    FLUJO DE PRUEBAS COMPLETO                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐                                                            │
│  │   SKILL     │                                                            │
│  │ pruebas-cli │                                                            │
│  └──────┬──────┘                                                            │
│         │                                                                   │
│         ▼                                                                   │
│  ┌─────────────┐     ┌─────────────┐     ┌───────────────────────────┐     │
│  │  MINIKUBE   │────▶│   HELM      │────▶│  HODEI OPERATOR + SERVER  │     │
│  │  (local)    │     │  (charts)   │     │  (CRDs + gRPC)            │     │
│  └─────────────┘     └─────────────┘     └───────────────────────────┘     │
│         │                                                                   │
│         │                                                                   │
│         ▼                                                                   │
│  ┌─────────────┐     ┌─────────────┐     ┌───────────────────────────┐     │
│  │   DEVSPACE  │────▶│   DEV MODE  │────▶│  HOT RELOAD + DEBUG       │     │
│  │  (dev)      │     │   (sync)    │     │  (iteración rápida)       │     │
│  └─────────────┘     └─────────────┘     └───────────────────────────┘     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 🔧 Fase 1: Preparación del Entorno

### Con Skill Pruebas-CLI

```bash
# Verificar herramientas instaladas
skill: pruebas-cli cluster status

# Si el cluster no existe, crearlo
skill: pruebas-cli cluster up
```

### Manualmente

```bash
# 1. Verificar que Minikube está instalado
minikube version

# 2. Verificar kubectl
kubectl version --client

# 3. Verificar Helm
helm version

# 4. Verificar DevSpace
devspace --version

# 5. Verificar Docker
docker --version
```

### Iniciar Minikube (si no existe)

```bash
# Crear cluster Minikube con recursos suficientes
minikube start \
  --cpus=4 \
  --memory=8g \
  --driver=docker \
  --kubernetes-version=v1.28.0

# Verificar estado
minikube status
```

---

## 🚀 Fase 2: Despliegue de Infraestructura

### Con Skill Pruebas-CLI

```bash
# Ver recursos del sistema Hodei
skill: pruebas-cli cluster resources

# Verificar que los servicios básicos están corriendo
skill: pruebas-cli cluster status
```

### Manualmente

```bash
# 1. Habilitar addons necesarios (si usas k3d en lugar de Minikube)
# minikube addons enable ingress
# minikube addons enable registry

# 2. Crear namespace para el sistema
kubectl create namespace hodei-system

# 3. Instalar PostgreSQL (usando el Helm chart del proyecto)
helm install postgres ./helm/hodei-jobs-platform \
  --namespace hodei-system \
  --set postgresql.enabled=true

# 4. Instalar NATS (JetStream)
helm install nats ./helm/hodei-jobs-platform \
  --namespace hodei-system \
  --set nats.enabled=true

# 5. Verificar pods
kubectl get pods -n hodei-system -w
```

### Esperar a que los servicios estén listos

```bash
# Verificar que PostgreSQL está listo
kubectl wait --for=condition=ready pod -l app=postgres -n hodei-system --timeout=120s

# Verificar que NATS está listo
kubectl wait --for=condition=ready pod -l app=nats -n hodei-system --timeout=120s

# Ver servicios
kubectl get svc -n hodei-system
```

---

## 📦 Fase 3: Despliegue del Operator

### Con Skill Pruebas-CLI

```bash
# Instalar el operator
skill: pruebas-cli operator install

# Verificar estado del operator
skill: pruebas-cli operator status

# Ver logs del operator
skill: pruebas-cli operator logs
```

### Manualmente

```bash
# 1. Configurar Docker para Minikube (para imágenes locales)
eval $(minikube docker-env)

# 2. Buildar la imagen del operator
docker build -t hodei/operator:latest ./crates/operator

# 3. Instalar el operator con Helm
helm install hodei-operator ./helm/operator \
  --namespace hodei-system \
  --set image.repository=hodei/operator \
  --set image.tag=latest \
  --set hodeiServer.address=http://hodei-server:50051 \
  --set crds.create=true

# 4. Verificar que el operator está corriendo
kubectl get pods -n hodei-system -l app=hodei-operator -w
```

### Verificar CRDs instalados

```bash
# Ver CRDs
kubectl get crds | grep hodei.io

# Expected output:
# jobs.hodei.io                          2024-01-01T00:00:00Z
# providerconfigs.hodei.io               2024-01-01T00:00:00Z
# workerpools.hodei.io                   2024-01-01T00:00:00Z
```

---

## 🔨 Fase 4: Desarrollo con DevSpace

### Desarrollo del Servidor

```bash
# Terminal 1: Desarrollo del servidor con hot-reload
devspace dev --profile development

# El servidor se sincroniza automáticamente
# Cambios en código triggers rebuild + restart
```

### Desarrollo del Operator

```bash
# Terminal 2: Desarrollo del operator
devspace dev --profile operator-dev

# Ver logs del operator en tiempo real
devspace logs-operator -f
```

### Desarrollo del Worker

```bash
# Terminal 3: Desarrollo del worker
devspace dev --profile worker-dev
```

### Comandos DevSpace útiles durante desarrollo

```bash
# Ver estado de deployments
devspace status

# Abrir terminal en el servidor
devspace terminal

# Puerto forwarding al servidor gRPC
devspace port-forward-server

# Puerto forwarding al operator (métricas)
devspace port-forward-operator

# Ver todos los logs
devspace logs -f
```

---

## 🧪 Fase 5: Pruebas End-to-End

### 1. Crear un Job CRD

```bash
# Crear un job de prueba
kubectl apply -f examples/crds/job-example.yaml -n hodei-system

# Verificar que el job fue creado
kubectl get jobs.hodei.io -n hodei-system -w
```

### 2. Verificar que el Operator procesa el Job

```bash
# Ver logs del operator (deberías ver la llamada gRPC)
kubectl logs -n hodei-system -l app=hodei-operator -f | grep -E "gRPC|Job|schedule"

# Ver eventos del job
kubectl describe job.hodei.io example-job -n hodei-system
```

### 3. Verificar el estado del Job

```bash
# Ver estado completo
kubectl get jobs.hodei.io example-job -n hodei-system -o yaml

# Ver eventos
kubectl get events -n hodei-system --field-selector involvedObject.name=example-job
```

### 4. Probar con WorkerPool

```bash
# Crear un worker pool
kubectl apply -f examples/crds/worker-pool-example.yaml -n hodei-system

# Ver worker pools
kubectl get workerpools.hodei.io -n hodei-system

# Ver detalles del worker pool
kubectl describe workerpool.hodei.io example-worker-pool -n hodei-system
```

### 5. Probar con ProviderConfig

```bash
# Crear provider config
kubectl apply -f examples/crds/provider-config-example.yaml -n hodei-system

# Ver provider configs
kubectl get providerconfigs.hodei.io -n hodei-system
```

### 6. Ejecutar pruebas con el skill

```bash
# Verificar jobs activos
skill: pruebas-cli job ls

# Ver estado de un job específico
skill: pruebas-cli job get example-job

# Ver logs del job
skill: pruebas-cli job logs example-job
```

---

## 🔍 Fase 6: Verificación y Debugging

### Verificación de Logs

```bash
# Logs del operador
kubectl logs -n hodei-system -l app=hodei-operator -f

# Logs del servidor (si está desplegado)
kubectl logs -n hodei-system -l app=hodei-server -f

# Logs del worker (si está desplegado)
kubectl logs -n hodei-system -l app=hodei-worker -f
```

### Debugging con Skill

```bash
# Verificar estado del cluster
skill: pruebas-cli cluster status

# Verificar estado del operator
skill: pruebas-cli operator status

# Ver logs del operator
skill: pruebas-cli operator logs

# Verificar worker pools
skill: pruebas-cli worker-pool ls
```

### Debugging de Jobs

```bash
# Ver estado del job
kubectl get jobs.hodei.io example-job -n hodei-system -o wide

# Ver detalles completos
kubectl describe job.hodei.io example-job -n hodei-system

# Ver eventos relacionados
kubectl get events -n hodei-system --sort-by='.metadata.creationTimestamp' | grep example-job
```

### Port-forwarding para Testing Manual

```bash
# Puerto forwarding al operador (para métricas Prometheus)
kubectl port-forward -n hodei-system svc/hodei-operator 8080:8080

# En otra terminal, verificar métricas
curl http://localhost:8080/metrics
```

---

## 📊 Comandos de Referencia Rápida

### Skilla Pruebas-CLI

| Acción | Comando |
|--------|---------|
| Verificar cluster | `skill: pruebas-cli cluster status` |
| Ver recursos | `skill: pruebas-cli cluster resources` |
| Instalar operator | `skill: pruebas-cli operator install` |
| Ver estado operator | `skill: pruebas-cli operator status` |
| Ver logs operator | `skill: pruebas-cli operator logs` |
| Listar jobs | `skill: pruebas-cli job ls` |
| Ver job | `skill: pruebas-cli job get <nombre>` |
| Listar worker pools | `skill: pruebas-cli worker-pool ls` |

### DevSpace

| Acción | Comando |
|--------|---------|
| Desarrollo server | `devspace dev --profile development` |
| Desarrollo operator | `devspace dev --profile operator-dev` |
| Desarrollo worker | `devspace dev --profile worker-dev` |
| Ver status | `devspace status` |
| Ver logs | `devspace logs -f` |
| Terminal | `devspace terminal` |
| Puerto forward server | `devspace port-forward-server` |

### kubectl/Helm

| Acción | Comando |
|--------|---------|
| Ver pods | `kubectl get pods -n hodei-system` |
| Ver CRDs | `kubectl get crds \| grep hodei.io` |
| Instalar operator | `helm install hodei-operator ./helm/operator` |
| Upgrade operator | `helm upgrade hodei-operator ./helm/operator` |
| Desinstalar | `helm uninstall hodei-operator -n hodei-system` |
| Eventos | `kubectl get events -n hodei-system --sort-by=.metadata.creationTimestamp` |

---

## 🐛 Solución de Problemas Comunes

### El operador no inicia

```bash
# Ver descripción del pod
kubectl describe pod -n hodei-system -l app=hodei-operator

# Ver logs anteriores
kubectl logs -n hodei-system -l app=hodei-operator --previous

# Verificar imagen
kubectl get pod -n hodei-system -l app=hodei-operator -o jsonpath='{.spec.containers[*].image}'
```

### El servidor gRPC no responde

```bash
# Verificar servicio
kubectl get svc hodei-server -n hodei-system

# Test de conectividad
kubectl exec -n hodei-system -it <operator-pod> -- curl http://hodei-server:50051/health

# Port-forward manual
kubectl port-forward -n hodei-system svc/hodei-server 50051:50051
```

### CRDs no se crean

```bash
# Verificar si crds.create está enabled
helm get values hodei-operator -n hodei-system

# Reinstalar con CRDs
helm uninstall hodei-operator -n hodei-system
helm install hodei-operator ./helm/operator \
  --namespace hodei-system \
  --set crds.create=true
```

### Error de conexión a base de datos

```bash
# Verificar secrets
kubectl get secrets -n hodei-system

# Verificar variables de entorno del pod
kubectl exec -n hodei-system <pod-name> -- env | grep -i postgres

# Test de conexión
kubectl exec -n hodei-system <pod-name> -- psql -h postgres -U postgres -d hodei_jobs -c "SELECT 1"
```

### Minikube sin memoria

```bash
# Ver uso de recursos
minikube ssh "docker stats"

# Detener servicios no esenciales
kubectl scale deployment postgres nats -n hodei-system --replicas=0

# Aumentar memoria (requiere reinicio)
minikube stop
minikube start --cpus=6 --memory=12g
```

---

## ✅ Checklist de Verificación Final

- [ ] Minikube está corriendo y saludable
- [ ] PostgreSQL está listo y accesible
- [ ] NATS/JetStream está corriendo
- [ ] Operator está instalado y en estado Running
- [ ] CRDs están creados (jobs.hodei.io, workerpools.hodei.io, providerconfigs.hodei.io)
- [ ] DevSpace está configurado y funcionando
- [ ] Job CRD se crea correctamente
- [ ] Operator procesa el Job (logs muestran llamada gRPC)
- [ ] WorkerPool CRD funciona
- [ ] ProviderConfig CRD funciona
- [ ] Skill pruebas-cli responde correctamente

---

**Document Version:** **Date:** 1.0  
2026-01-10  
**Author:** Hodei Jobs Platform Team
