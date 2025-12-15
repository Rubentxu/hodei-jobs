# Hodei Jobs Platform - Guía de Despliegue

Esta guía cubre el despliegue de Hodei Jobs Platform en diferentes entornos.

## Tabla de Contenidos

- [Requisitos Previos](#requisitos-previos)
- [Despliegue con Docker Compose](#despliegue-con-docker-compose)
- [Despliegue en Kubernetes con Helm](#despliegue-en-kubernetes-con-helm)
- [Variables de Entorno](#variables-de-entorno)
- [Configuración de Producción](#configuración-de-producción)
- [Monitorización](#monitorización)
- [Troubleshooting](#troubleshooting)

---

## Requisitos Previos

### Para Docker Compose

- Docker Engine 24.0+
- Docker Compose v2.20+
- 4GB RAM mínimo
- 20GB espacio en disco

### Para Kubernetes

- Kubernetes 1.28+
- Helm 3.12+
- kubectl configurado
- Ingress Controller (nginx recomendado)
- 8GB RAM mínimo en el cluster

---

## Despliegue con Docker Compose

### 1. Desarrollo Local

```bash
# Clonar repositorio
git clone https://github.com/Rubentxu/hodei-jobs.git
cd hodei-jobs

# Levantar solo PostgreSQL
docker compose -f docker-compose.dev.yml up -d

# Ejecutar backend localmente
HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei" \
HODEI_DEV_MODE=1 \
HODEI_DOCKER_ENABLED=1 \
cargo run --bin server -p hodei-jobs-grpc

# En otra terminal, ejecutar frontend
cd web
VITE_API_URL=http://localhost:50051 bun run dev
```

### 2. Producción con Docker Compose

```bash
# Crear archivo .env
cat > .env << EOF
POSTGRES_USER=hodei
POSTGRES_PASSWORD=tu-password-seguro
POSTGRES_DB=hodei
HODEI_OTP_SECRET=tu-secreto-otp-32-caracteres
VERSION=1.0.0
EOF

# Construir imágenes
docker compose -f docker-compose.prod.yml build

# Levantar stack completo
docker compose -f docker-compose.prod.yml up -d

# Ver logs
docker compose -f docker-compose.prod.yml logs -f

# Verificar estado
docker compose -f docker-compose.prod.yml ps
```

### 3. Con Monitorización (Prometheus + Grafana)

```bash
# Levantar con perfil de monitorización
docker compose -f docker-compose.prod.yml --profile monitoring up -d

# Acceder a:
# - Web Dashboard: http://localhost
# - Prometheus: http://localhost:9090
# - Grafana: http://localhost:3000 (admin/admin)
```

### 4. Comandos Útiles

```bash
# Parar todo
docker compose -f docker-compose.prod.yml down

# Parar y eliminar volúmenes (¡CUIDADO! Borra datos)
docker compose -f docker-compose.prod.yml down -v

# Reiniciar un servicio
docker compose -f docker-compose.prod.yml restart api

# Ver logs de un servicio
docker compose -f docker-compose.prod.yml logs -f api

# Escalar workers (si aplica)
docker compose -f docker-compose.prod.yml up -d --scale api=3
```

---

## Despliegue en Kubernetes con Helm

### 1. Preparación

```bash
# Añadir repositorio de Bitnami (para PostgreSQL)
helm repo add bitnami https://charts.bitnami.com/bitnami
helm repo update

# Crear namespace
kubectl create namespace hodei
```

### 2. Crear Namespace para Workers

```bash
kubectl create namespace hodei-workers
```

### 3. Construir y Subir Imágenes

```bash
# Construir imágenes
docker build -t your-registry/hodei-server:1.0.0 .
docker build -t your-registry/hodei-web:1.0.0 ./web
docker build -t your-registry/hodei-worker:1.0.0 -f scripts/docker/Dockerfile.worker .

# Subir a registry
docker push your-registry/hodei-server:1.0.0
docker push your-registry/hodei-web:1.0.0
docker push your-registry/hodei-worker:1.0.0
```

### 4. Instalar con Helm

```bash
# Instalación básica
helm install hodei ./deploy/helm/hodei-jobs \
  --namespace hodei \
  --set postgresql.auth.password=tu-password-seguro \
  --set security.otpSecret=tu-secreto-otp-32-caracteres \
  --set global.imageRegistry=your-registry/

# Con Ingress habilitado
helm install hodei ./deploy/helm/hodei-jobs \
  --namespace hodei \
  --set postgresql.auth.password=tu-password-seguro \
  --set security.otpSecret=tu-secreto-otp-32-caracteres \
  --set global.imageRegistry=your-registry/ \
  --set web.ingress.enabled=true \
  --set web.ingress.hosts[0].host=hodei.tu-dominio.com \
  --set web.ingress.hosts[0].paths[0].path=/ \
  --set web.ingress.hosts[0].paths[0].pathType=Prefix
```

### 5. Verificar Instalación

```bash
# Ver pods
kubectl get pods -n hodei

# Ver servicios
kubectl get svc -n hodei

# Ver logs del API
kubectl logs -n hodei -l app.kubernetes.io/component=api -f

# Port-forward para acceso local
kubectl port-forward -n hodei svc/hodei-api 50051:50051
kubectl port-forward -n hodei svc/hodei-web 8080:80
```

### 6. Actualizar Despliegue

```bash
# Actualizar valores
helm upgrade hodei ./deploy/helm/hodei-jobs \
  --namespace hodei \
  --reuse-values \
  --set api.image.tag=1.1.0

# Rollback si hay problemas
helm rollback hodei -n hodei
```

### 7. Desinstalar

```bash
helm uninstall hodei -n hodei
kubectl delete namespace hodei
kubectl delete namespace hodei-workers
```

---

## Variables de Entorno

### Backend (API Server)


| Variable               | Descripción                   | Valor por Defecto     | Requerido |
| ---------------------- | ------------------------------ | --------------------- | --------- |
| `HODEI_DATABASE_URL`   | URL de conexión PostgreSQL    | -                     | ✅        |
| `HODEI_DEV_MODE`       | Modo desarrollo (0/1)          | `0`                   | ❌        |
| `HODEI_DOCKER_ENABLED` | Habilitar Docker Provider      | `0`                   | ❌        |
| `HODEI_K8S_ENABLED`    | Habilitar Kubernetes Provider  | `0`                   | ❌        |
| `HODEI_FC_ENABLED`     | Habilitar Firecracker Provider | `0`                   | ❌        |
| `HODEI_OTP_SECRET`     | Secreto para generar OTPs      | -                     | ✅ (prod) |
| `HODEI_WORKER_IMAGE`   | Imagen Docker para workers     | `hodei-worker:latest` | ❌        |
| `HODEI_K8S_NAMESPACE`  | Namespace para pods worker     | `hodei-workers`       | ❌        |
| `GRPC_PORT`            | Puerto gRPC                    | `50051`               | ❌        |
| `RUST_LOG`             | Nivel de logging               | `info`                | ❌        |

### Frontend (Web Dashboard)


| Variable        | Descripción         | Valor por Defecto        | Requerido |
| --------------- | -------------------- | ------------------------ | --------- |
| `VITE_API_URL`  | URL del backend gRPC | `http://localhost:50051` | ✅        |
| `VITE_USE_MOCK` | Usar datos mock      | `false`                  | ❌        |

### PostgreSQL


| Variable            | Descripción      | Valor por Defecto |
| ------------------- | ----------------- | ----------------- |
| `POSTGRES_USER`     | Usuario de BD     | `hodei`           |
| `POSTGRES_PASSWORD` | Contraseña de BD | - (requerido)     |
| `POSTGRES_DB`       | Nombre de BD      | `hodei`           |

---

## Configuración de Producción

### Checklist de Seguridad

- [ ]  Cambiar `POSTGRES_PASSWORD` por un valor seguro
- [ ]  Configurar `HODEI_OTP_SECRET` con 32+ caracteres aleatorios
- [ ]  Deshabilitar `HODEI_DEV_MODE` (establecer a `0`)
- [ ]  Configurar TLS/HTTPS en el Ingress
- [ ]  Limitar acceso a puertos internos
- [ ]  Configurar backups de PostgreSQL
- [ ]  Revisar límites de recursos (CPU/memoria)

### Generar Secretos Seguros

```bash
# Generar password PostgreSQL
openssl rand -base64 32

# Generar OTP secret
openssl rand -hex 32
```

### Configurar TLS con cert-manager

```yaml
# values-production.yaml
web:
  ingress:
    enabled: true
    className: nginx
    annotations:
      cert-manager.io/cluster-issuer: letsencrypt-prod
    hosts:
      - host: hodei.tu-dominio.com
        paths:
          - path: /
            pathType: Prefix
    tls:
      - secretName: hodei-tls
        hosts:
          - hodei.tu-dominio.com
```

---

## Monitorización

### Métricas Disponibles

El servidor expone métricas en formato Prometheus:

- `hodei_jobs_total` - Total de jobs procesados
- `hodei_jobs_duration_seconds` - Duración de ejecución
- `hodei_workers_active` - Workers activos
- `hodei_queue_size` - Tamaño de la cola

### Configurar ServiceMonitor (Prometheus Operator)

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: hodei-api
  namespace: hodei
spec:
  selector:
    matchLabels:
      app.kubernetes.io/component: api
  endpoints:
    - port: metrics
      interval: 30s
```

---

## Troubleshooting

### El API no arranca

```bash
# Verificar logs
docker compose -f docker-compose.prod.yml logs api

# Verificar conexión a PostgreSQL
docker compose -f docker-compose.prod.yml exec postgres pg_isready

# Verificar variables de entorno
docker compose -f docker-compose.prod.yml exec api env | grep HODEI
```

### Workers no se registran

```bash
# Verificar que Docker está disponible
docker ps

# Verificar logs del worker
docker logs hodei-worker-<id>

# Verificar OTP secret coincide
echo $HODEI_OTP_SECRET
```

### Frontend no conecta con API

```bash
# Verificar que el API está corriendo
curl http://localhost:50051/health

# Verificar CORS está configurado
# El servidor debe aceptar requests desde el origen del frontend
```

### Problemas de memoria

```bash
# Ver uso de recursos
docker stats

# Ajustar límites en docker-compose.prod.yml
deploy:
  resources:
    limits:
      memory: 2G
```

---

## Soporte

- **Issues**: https://github.com/Rubentxu/hodei-jobs/issues
- **Documentación**: https://github.com/Rubentxu/hodei-jobs/docs
