# Hodei Jobs Platform - Manual de Usuario

Guía completa para usuarios de la plataforma Hodei Jobs.

## Tabla de Contenidos

- [Introducción](#introducción)
- [Acceso al Dashboard](#acceso-al-dashboard)
- [Dashboard Principal](#dashboard-principal)
- [Gestión de Jobs](#gestión-de-jobs)
- [Monitorización de Logs](#monitorización-de-logs)
- [Métricas del Sistema](#métricas-del-sistema)
- [Gestión de Providers](#gestión-de-providers)
- [Uso de la CLI](#uso-de-la-cli)
- [API gRPC](#api-grpc)
- [FAQ](#faq)

---

## Introducción

**Hodei Jobs** es una plataforma de ejecución de jobs distribuida que provisiona workers automáticamente bajo demanda. Soporta múltiples proveedores de infraestructura:

- **Docker**: Contenedores para desarrollo y CI/CD
- **Kubernetes**: Pods para producción y auto-escalado
- **Firecracker**: microVMs para máximo aislamiento

### Conceptos Clave

| Concepto | Descripción |
|----------|-------------|
| **Job** | Unidad de trabajo a ejecutar (comando, script) |
| **Worker** | Contenedor/pod que ejecuta jobs |
| **Provider** | Infraestructura que provisiona workers |
| **Queue** | Cola de jobs pendientes de ejecución |

---

## Acceso al Dashboard

### URL de Acceso

- **Desarrollo**: http://localhost:5173
- **Producción**: https://hodei.tu-dominio.com

### Navegación

El dashboard tiene una barra de navegación inferior con 4 secciones:

| Icono | Sección | Descripción |
|-------|---------|-------------|
| 🏠 | Dashboard | Vista general del sistema |
| 📋 | Jobs | Historial y gestión de jobs |
| 📊 | Metrics | Métricas y estadísticas |
| 🖥️ | Providers | Gestión de proveedores |

---

## Dashboard Principal

La página principal muestra un resumen del estado del sistema.

### Stats Cards

| Card | Descripción |
|------|-------------|
| **Total Jobs** | Número total de jobs procesados |
| **Running** | Jobs actualmente en ejecución (animado) |
| **Failed** | Jobs que han fallado |
| **Success** | Jobs completados exitosamente |

### System Health

Gráfico que muestra:
- **CPU Load**: Carga promedio del sistema
- **Uptime**: Tiempo de actividad

### Recent Executions

Lista de los últimos jobs ejecutados con:
- ID del job
- Nombre
- Estado (Running, Success, Failed)
- Duración
- Hora de inicio

---

## Gestión de Jobs

### Ver Historial de Jobs

1. Navegar a **Jobs** en la barra inferior
2. Usar los filtros para buscar:
   - **All Jobs**: Todos los jobs
   - **Running**: Solo en ejecución
   - **Failed**: Solo fallidos
   - **Completed**: Solo completados
3. Usar la barra de búsqueda para filtrar por nombre o ID

### Ver Detalles de un Job

1. Click en cualquier job de la lista
2. Se muestra:
   - **Estado actual** con indicador visual
   - **Progreso** (si está en ejecución)
   - **Comando ejecutado**
   - **Tiempos** (inicio, fin, duración)
   - **Recursos** (CPU, memoria)
   - **Logs** en tiempo real

### Crear un Nuevo Job

1. Click en el botón **+ New Job**
2. Completar el formulario:
   - **Job Name**: Nombre descriptivo
   - **Command**: Comando a ejecutar
   - **Arguments**: Argumentos (opcional)
   - **Environment**: Variables de entorno
   - **Priority**: Normal, High, Critical
   - **Timeout**: Tiempo máximo de ejecución
3. Click en **Schedule Job**

### Cancelar un Job

1. Ir a los detalles del job
2. Click en **Cancel Job**
3. Confirmar la acción

---

## Monitorización de Logs

### Ver Logs en Tiempo Real

1. Navegar a un job en ejecución
2. Los logs se muestran automáticamente
3. Controles disponibles:
   - **Pause/Resume**: Pausar el stream
   - **Clear**: Limpiar la vista
   - **Download**: Descargar logs

### Niveles de Log

| Nivel | Color | Descripción |
|-------|-------|-------------|
| INFO | Blanco | Información general |
| WARN | Amarillo | Advertencias |
| ERROR | Rojo | Errores |
| DEBUG | Gris | Depuración (solo dev) |

### Filtrar Logs

- Usar la barra de búsqueda para filtrar por texto
- Seleccionar nivel mínimo de log

---

## Métricas del Sistema

### Selector de Rango de Tiempo

| Opción | Período |
|--------|---------|
| 1H | Última hora |
| 24H | Últimas 24 horas |
| 7D | Últimos 7 días |
| 30D | Últimos 30 días |

### KPIs Principales

| KPI | Descripción |
|-----|-------------|
| **Total Jobs** | Jobs en el período seleccionado |
| **Success Rate** | Porcentaje de éxito |
| **Avg Duration** | Duración promedio |
| **CPU Load** | Carga de CPU promedio |

### Gráficos

- **Job Distribution**: Distribución por estado (pie chart)
- **Execution Trends**: Tendencia de ejecuciones (line chart)
- **Active Providers**: Estado de los proveedores

---

## Gestión de Providers

### Ver Providers

1. Navegar a **Providers**
2. Ver lista de proveedores configurados
3. Filtrar por:
   - **All**: Todos
   - **Active**: Activos
   - **Unhealthy**: Con problemas

### Información del Provider

| Campo | Descripción |
|-------|-------------|
| **Name** | Nombre del proveedor |
| **Type** | Docker, K8s, Firecracker |
| **Status** | Active, Unhealthy, Offline |
| **Current Jobs** | Jobs en ejecución |
| **Health Score** | Puntuación de salud (0-100) |

### Ver Detalles del Provider

1. Click en un provider
2. Ver:
   - **Configuration**: Endpoint, recursos
   - **Capabilities**: CPU, memoria, GPU
   - **Health Log**: Historial de estado
   - **Raw Config**: Configuración JSON

### Acciones del Provider

| Acción | Descripción |
|--------|-------------|
| **Mark Healthy** | Marcar como saludable |
| **Maintenance** | Poner en mantenimiento |
| **Shutdown** | Apagar el provider |

### Registrar Nuevo Provider

1. Click en **+ Add Provider**
2. Seleccionar tipo (Docker, K8s, Azure VM)
3. Configurar:
   - **Name**: Nombre único
   - **Endpoint URL**: URL de conexión
   - **API Token**: Token de autenticación
   - **Labels**: Etiquetas para scheduling
4. Configurar capacidades:
   - **Max Memory**: Memoria máxima
   - **Max vCPUs**: CPUs virtuales
   - **GPU Support**: Soporte GPU
5. Click en **Create Provider**

---

## Uso de la CLI

### Instalación

```bash
# Desde el binario compilado
cargo install --path crates/cli

# O usar directamente
cargo run --bin hodei-jobs-cli -- <comando>
```

### Comandos Disponibles

```bash
# Ver ayuda
hodei-jobs-cli --help

# Verificar salud del servidor
hodei-jobs-cli health

# Listar jobs
hodei-jobs-cli jobs list

# Ver detalles de un job
hodei-jobs-cli jobs get <job-id>

# Encolar un job
hodei-jobs-cli jobs queue --name "Mi Job" --command "echo" --args "Hello"

# Cancelar un job
hodei-jobs-cli jobs cancel <job-id>

# Ver logs de un job
hodei-jobs-cli logs <job-id> --follow

# Listar workers
hodei-jobs-cli workers list

# Ver estado de la cola
hodei-jobs-cli queue status
```

### Ejemplos

```bash
# Job simple
hodei-jobs-cli jobs queue \
  --name "Build Project" \
  --command "npm" \
  --args "run build"

# Job con variables de entorno
hodei-jobs-cli jobs queue \
  --name "Deploy" \
  --command "./deploy.sh" \
  --env "ENV=production" \
  --env "VERSION=1.0.0"

# Job con timeout
hodei-jobs-cli jobs queue \
  --name "Long Task" \
  --command "python" \
  --args "process.py" \
  --timeout 3600
```

---

## API gRPC

### Endpoints Disponibles

| Servicio | Método | Descripción |
|----------|--------|-------------|
| `JobExecutionService` | `QueueJob` | Encolar un job |
| `JobExecutionService` | `CancelJob` | Cancelar un job |
| `JobExecutionService` | `StartJob` | Iniciar un job |
| `JobExecutionService` | `CompleteJob` | Marcar como completado |
| `LogStreamService` | `SubscribeLogs` | Stream de logs |
| `LogStreamService` | `GetLogs` | Obtener logs históricos |
| `MetricsService` | `GetAggregatedMetrics` | Métricas agregadas |
| `SchedulerService` | `GetQueueStatus` | Estado de la cola |
| `WorkerAgentService` | `Register` | Registrar worker |

### Ejemplo con grpcurl

```bash
# Encolar job
grpcurl -plaintext -d '{
  "job_definition": {
    "job_id": {"value": "job-001"},
    "name": "Test Job",
    "command": "echo",
    "arguments": ["Hello World"]
  },
  "queued_by": "user"
}' localhost:50051 hodei.JobExecutionService/QueueJob

# Ver estado de cola
grpcurl -plaintext -d '{
  "scheduler_name": "default"
}' localhost:50051 hodei.SchedulerService/GetQueueStatus

# Stream de logs
grpcurl -plaintext -d '{
  "job_id": "job-001",
  "include_history": true
}' localhost:50051 hodei.LogStreamService/SubscribeLogs
```

---

## FAQ

### ¿Cómo sé si un job está ejecutándose?

El job tendrá estado **RUNNING** y mostrará un indicador animado en el dashboard.

### ¿Qué pasa si un job falla?

1. El estado cambia a **FAILED**
2. Se registra el mensaje de error
3. Puedes ver los logs para diagnosticar
4. Opcionalmente, puedes reintentar el job

### ¿Cuánto tiempo se guardan los logs?

Por defecto, los logs se mantienen 7 días. Configurable via `HODEI_LOG_RETENTION_DAYS`.

### ¿Puedo ejecutar jobs en paralelo?

Sí, el sistema ejecuta jobs en paralelo según la capacidad de los workers disponibles.

### ¿Cómo escalo el sistema?

- **Docker**: Aumentar recursos del contenedor
- **Kubernetes**: Aumentar réplicas del API y workers
- **Workers**: Registrar más providers

### ¿Qué imagen usan los workers?

Por defecto `hodei-worker:latest`. Configurable via `HODEI_WORKER_IMAGE`.

### ¿Cómo configuro prioridades?

Los jobs con prioridad **CRITICAL** se ejecutan primero, seguidos de **HIGH**, **NORMAL**, y **LOW**.

### ¿Hay límite de jobs en cola?

No hay límite por defecto. Configurable via `HODEI_MAX_QUEUE_SIZE`.

---

## Soporte

- **Documentación**: https://github.com/Rubentxu/hodei-jobs/docs
- **Issues**: https://github.com/Rubentxu/hodei-jobs/issues
- **Discusiones**: https://github.com/Rubentxu/hodei-jobs/discussions
