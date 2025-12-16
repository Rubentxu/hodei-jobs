# Guía de Usuario - Hodei Jobs Platform

**Versión**: 8.0
**Última Actualización**: 2025-12-16

Guía práctica para usuarios que quieren ejecutar jobs distribuidos usando la interfaz web de Hodei Jobs Platform.

## 📋 Índice

1. [Inicio Rápido](#-inicio-rápido)
2. [Levantar la Aplicación](#-levantar-la-aplicación)
3. [Interfaz Web](#-interfaz-web)
   - [Dashboard](#dashboard)
   - [Crear un Job](#crear-un-job)
   - [Ver Detalles de un Job](#ver-detalles-de-un-job)
   - [Logs en Tiempo Real](#logs-en-tiempo-real)
   - [Historial de Jobs](#historial-de-jobs)
   - [Gestión de Providers](#gestión-de-providers)
   - [Métricas del Sistema](#métricas-del-sistema)
4. [Ejemplos Prácticos](#-ejemplos-prácticos)
5. [Arquitectura del Sistema](#-arquitectura-del-sistema)
6. [Troubleshooting](#-troubleshooting)
7. [Referencia para Desarrolladores](#-referencia-para-desarrolladores)

---

## 🚀 Inicio Rápido

En menos de 5 minutos puedes tener la plataforma funcionando y ejecutar tu primer job.

### Requisitos

- **Docker** y **Docker Compose** instalados
- Puerto `80` disponible (web)
- Puerto `50051` disponible (API gRPC)
- Puerto `5432` disponible (PostgreSQL)

### Pasos Rápidos

```bash
# 1. Clonar el repositorio
git clone <repo-url>
cd hodei-jobs

# 2. Configurar variables de entorno
cat > .env << EOF
POSTGRES_PASSWORD=secure_password_here
EOF

# 3. Levantar toda la plataforma
docker compose -f docker-compose.prod.yml up -d

# 4. Abrir la interfaz web
open http://localhost  # macOS
xdg-open http://localhost  # Linux
```

¡Listo! Ya puedes crear y ejecutar jobs desde la interfaz web.

## 🐳 Levantar la Aplicación

### Opción 1: Producción (Recomendado)

Levanta toda la plataforma con un solo comando:

```bash
# Crear archivo de configuración
cat > .env << EOF
POSTGRES_PASSWORD=tu-password-seguro
GRAFANA_PASSWORD=admin
EOF

# Levantar servicios principales
docker compose -f docker-compose.prod.yml up -d --build

# Ver logs
docker compose -f docker-compose.prod.yml logs -f

# Con monitoreo (Prometheus + Grafana)
docker compose -f docker-compose.prod.yml --profile monitoring up -d
```

**Servicios disponibles:**

| Servicio          | URL                   | Descripción           |
| ----------------- | --------------------- | --------------------- |
| **Web Dashboard** | http://localhost      | Interfaz principal    |
| **API gRPC**      | localhost:50051       | API para clientes     |
| **PostgreSQL**    | localhost:5432        | Base de datos         |
| **Prometheus**    | http://localhost:9090 | Métricas (opcional)   |
| **Grafana**       | http://localhost:3000 | Dashboards (opcional) |

### Opción 2: Desarrollo Local (Optimizado)

Hemos simplificado el flujo de desarrollo para que sea ultra-rápido.

```bash
# 1. Setup inicial (solo la primera vez)
./setup.sh

# 2. Iniciar entorno de desarrollo
./dev.sh
```

El script `./dev.sh` levantará automáticamente:

- PostgreSQL (en Docker)
- Backend (con Hot Reload via Bacon)
- Frontend (con HMR via Vite)

También puedes usar comandos individuales si lo prefieres:

```bash
./dev.sh db       # Solo base de datos
./dev.sh backend  # Solo backend
./dev.sh frontend # Solo frontend
```

### Verificar que todo funciona

```bash
# Ver estado de los contenedores
docker compose -f docker-compose.prod.yml ps

# Verificar API gRPC
grpcurl -plaintext localhost:50051 list

# Abrir la web
open http://localhost
```

---

## 🖥️ Interfaz Web

La interfaz web está diseñada para ser intuitiva y móvil-first. Aquí te explicamos cada sección.

### Dashboard

**URL:** `/` (página principal)

El dashboard muestra un resumen del estado del sistema:

- **Total Jobs**: Número total de jobs ejecutados
- **Running**: Jobs en ejecución actualmente
- **Failed**: Jobs que fallaron
- **Success**: Jobs completados exitosamente
- **System Health**: Carga de CPU y estado de los nodos
- **Recent Executions**: Últimos 5 jobs ejecutados

**Acciones rápidas:**

- Clic en el botón **+** (azul, esquina inferior derecha) para crear un nuevo job
- Clic en "See All" para ver el historial completo
- Clic en cualquier job reciente para ver sus detalles

---

### Crear un Job

**URL:** `/jobs/new`

Formulario completo para programar un nuevo job:

#### 1. Basic Info

- **Job Name**: Nombre descriptivo del job (ej: "Data Processing Pipeline")

#### 2. Core Execution

- **Command Type**: Tipo de comando a ejecutar
  - `Shell Command`: Comandos bash/shell
  - `Docker Exec`: Ejecutar dentro de un contenedor
  - `Python Script`: Script Python
  - `Node.js Script`: Script Node.js
- **Command / Script Content**: El comando o script a ejecutar

#### 3. Environment & Image

- **Container Image**: Imagen Docker a usar (ej: `ubuntu:latest`, `python:3.9`)
- **Environment Variables**: Variables de entorno (clave=valor)

#### 4. Resources

- **CPU Cores**: Número de cores (1-16)
- **Memory (MB)**: Memoria RAM en MB
- **Storage (MB)**: Almacenamiento temporal
- **Timeout (ms)**: Tiempo máximo de ejecución
- **GPU Required**: Activar si necesitas GPU
- **Architecture**: `x86_64` o `arm64`

#### 5. Preferences

- **Provider**: Seleccionar provider específico o "Any"
- **Region**: Región preferida o "Auto"
- **Job Priority**: `Low`, `Normal`, o `High`
- **Allow Retry**: Reintentar automáticamente si falla

**Ejemplo rápido:**

```
Job Name: Hello World Test
Command Type: Shell Command
Script: echo "Hello from Hodei!" && date
Container Image: alpine:latest
CPU Cores: 1
Memory: 512
```

Clic en **"Schedule Job"** para encolar el job.

---

### Ver Detalles de un Job

**URL:** `/jobs/:jobId`

Muestra información detallada de un job específico:

#### Pestañas disponibles:

**Overview:**

- **Timeline**: Progreso del job (Queued → Image Pulled → Running → Cleanup)
- **Live Resources**: Uso de CPU y memoria en tiempo real
- **Latest Logs**: Vista previa de los últimos logs

**Config:**

- Comando ejecutado
- Imagen utilizada
- Límites de CPU y memoria

**Logs:**

- Enlace al visor de logs completo

**Resources:**

- Gráficos detallados de uso de recursos

#### Acciones:

- **SSH Access**: Acceso directo al worker (si está disponible)
- **Cancel Job**: Cancelar el job en ejecución

---

### Logs en Tiempo Real

**URL:** `/jobs/:jobId/logs`

Visor de logs estilo terminal con streaming en tiempo real:

**Características:**

- **Búsqueda**: Filtrar logs por texto (grep)
- **Filtros por nivel**: All, INFO, WARN, ERROR
- **Pause/Resume**: Pausar el streaming para analizar
- **Auto-scroll**: Seguir automáticamente los nuevos logs

**Colores de logs:**

- 🔵 **INFO**: Información general (azul)
- 🟡 **WARN**: Advertencias (amarillo)
- 🔴 **ERROR**: Errores (rojo)

**Controles:**

- **Pause/Resume**: Pausar o continuar el streaming
- **Clear**: Limpiar la pantalla
- **Scroll to bottom**: Ir al final de los logs

---

### Historial de Jobs

**URL:** `/jobs`

Lista completa de todos los jobs con:

- ID del job
- Nombre
- Estado (Running, Success, Failed)
- Tiempo de ejecución
- Fecha de creación

**Filtros disponibles:**

- Por estado
- Por fecha
- Por nombre

---

### Gestión de Providers

**URL:** `/providers`

Lista de providers de infraestructura disponibles:

| Provider        | Descripción                                   |
| --------------- | --------------------------------------------- |
| **Docker**      | Ejecuta jobs en contenedores Docker locales   |
| **Kubernetes**  | Ejecuta jobs como Pods en un cluster K8s      |
| **Firecracker** | Ejecuta jobs en microVMs (máximo aislamiento) |

**Acciones:**

- Ver detalles de cada provider
- Habilitar/deshabilitar providers
- Configurar parámetros específicos

#### Crear nuevo Provider

**URL:** `/providers/new`

Formulario para agregar un nuevo provider con su configuración específica.

---

### Métricas del Sistema

**URL:** `/metrics`

Dashboard de métricas del sistema:

- Jobs por estado
- Tiempo promedio de ejecución
- Uso de recursos por provider
- Tendencias históricas

---

## 📝 Ejemplos Prácticos

### Ejemplo 1: Job Simple (Echo)

1. Ir a `/jobs/new`
2. Configurar:
   - **Job Name**: `Hello World`
   - **Command Type**: `Shell Command`
   - **Script**: `echo "Hello from Hodei!" && date`
   - **Container Image**: `alpine:latest`
3. Clic en **Schedule Job**
4. Ver el progreso en `/jobs/:jobId`

### Ejemplo 2: Script Python

1. Ir a `/jobs/new`
2. Configurar:
   - **Job Name**: `Python Data Processing`
   - **Command Type**: `Python Script`
   - **Script**:
     ```python
     import sys
     print("Processing data...")
     for i in range(5):
         print(f"Step {i+1}/5 completed")
     print("Done!")
     ```
   - **Container Image**: `python:3.11-slim`
   - **Memory**: `1024`
3. Clic en **Schedule Job**

### Ejemplo 3: Job con Variables de Entorno

1. Ir a `/jobs/new`
2. Configurar:
   - **Job Name**: `API Data Fetch`
   - **Command Type**: `Shell Command`
   - **Script**: `curl -H "Authorization: Bearer $API_TOKEN" $API_URL`
   - **Container Image**: `curlimages/curl:latest`
   - **Environment Variables**:
     - `API_TOKEN` = `tu-token-secreto`
     - `API_URL` = `https://api.example.com/data`
3. Clic en **Schedule Job**

### Ejemplo 4: Job de Larga Duración

1. Ir a `/jobs/new`
2. Configurar:
   - **Job Name**: `Long Running Task`
   - **Command Type**: `Shell Command`
   - **Script**:
     ```bash
     for i in $(seq 1 60); do
       echo "[$(date)] Processing batch $i/60..."
       sleep 1
     done
     echo "All batches completed!"
     ```
   - **Container Image**: `alpine:latest`
   - **Timeout**: `120000` (2 minutos)
3. Clic en **Schedule Job**
4. Ir a `/jobs/:jobId/logs` para ver el progreso en tiempo real

---

## 🔍 Verificación Avanzada (Eventos y Auditoría)

Para asegurar que los jobs se están ejecutando correctamente y generando los eventos de dominio esperados, puedes consultar directamente el sistema de auditoría.

### Consultar Logs de Auditoría (SQL)

Conéctate a la base de datos PostgreSQL corriendo en Docker.
_Nota: El usuario por defecto en desarrollo es `postgres`, en producción suele ser `hodei`._

```bash
# Opción A: Entorno Desarrollo
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei

# Opción B: Entorno Producción (o Inicio Rápido)
docker exec -it hodei-jobs-postgres psql -U hodei -d hodei

# Ejecutar query directa:
docker exec hodei-jobs-postgres psql -U hodei -d hodei -c "SELECT * FROM audit_logs LIMIT 5;"
```

### Consultas Útiles

#### 1. Ver últimos eventos registrados

Verifica qué está pasando en el sistema en tiempo real.

```sql
SELECT occurred_at, event_type, actor, payload
FROM audit_logs
ORDER BY occurred_at DESC
LIMIT 10;
```

#### 2. Seguir el ciclo de vida de un Job específico

Usando el `correlation_id` (que suele ser el Job ID para eventos de Job), puedes ver toda la historia de un job.

```sql
-- Reemplaza 'JOB_ID_AQUI' con el ID real de tu job
SELECT occurred_at, event_type, payload
FROM audit_logs
WHERE correlation_id = 'JOB_ID_AQUI'
ORDER BY occurred_at ASC;
```

> [!NOTE]
> Gracias a las mejoras recientes, **todos** los eventos del ciclo de vida (incluyendo éxito/fallo) ahora incluyen el `correlation_id`, facilitando el seguimiento completo con esta única query.

### Verificación del Ciclo de Vida (Orden de Eventos)

Para certificar que el flujo funciona correctamente, el orden cronológico de los eventos debe ser:

1.  **`JobCreated`**: El job entra al sistema (estado `Queued`).
2.  **`JobAssigned`**: El Scheduler asigna un worker.
3.  **`JobStatusChanged`** (Scheduled -> Running): El worker confirma el inicio de la ejecución.
4.  **`JobStatusChanged`** (Running -> Succeeded/Failed): El worker reporta la finalización.

### Verificación de Limpieza del Worker

Para verificar que el worker se libera correctamente tras finalizar el job, busca los eventos de latido (`WorkerHeartbeat`) o consulta el estado del worker. En entornos dinámicos (Docker), deberías ver que el contenedor se detiene y elimina si la política de escalado así lo dicta.

**Verificar liberación en logs del servidor:**

```bash
docker compose -f docker-compose.dev.yml logs api | grep "released"
```

### Verificación de Logs de Ejecución

Para confirmar que la salida del job (`stdout`/`stderr`) se transmite y registra correctamente:

1.  **Logs del Contenedor Worker** (si aún existe):

    ```bash
    # Listar contenedores de workers (incluso detenidos)
    docker ps -a --filter "name=hodei-worker"

    # Ver logs específicos
    docker logs <CONTAINER_ID>
    ```

2.  **Confirmar Recepción en el Servidor**:
    El servidor recibe los logs vía gRPC y los registra (nivel DEBUG/INFO).
    ```bash
    docker logs hodei-jobs-api 2>&1 | grep "Log appended"
    ```

#### 3. Estadísticas de Ejecución

Cuenta cuántos jobs han sido creados vs completados.

```sql
SELECT event_type, COUNT(*) as total
FROM audit_logs
GROUP BY event_type
ORDER BY total DESC;
```

### Verificación de Integridad

Si un job parece "atascado", busca si falta alguno de los eventos intermedios. Por ejemplo, si ves `JobCreated` pero nunca `JobAssigned`, el problema está en el Scheduler o en la falta de recursos (Providers).

```sql
-- Buscar jobs huérfanos (creados hace más de 5 min sin asignar)
SELECT * FROM audit_logs
WHERE event_type = 'JobCreated'
AND occurred_at < NOW() - INTERVAL '5 minutes'
AND correlation_id NOT IN (
    SELECT correlation_id FROM audit_logs WHERE event_type = 'JobAssigned'
);
```

---

## 🏗️ Arquitectura del Sistema

### Componentes

```
┌─────────────────────────────────────────────────────────────────┐
│                    HODEI JOBS PLATFORM                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐     ┌─────────────────────────────────────┐   │
│  │   Web UI    │────▶│         gRPC Server (API)           │   │
│  │  (React)    │     │                                     │   │
│  └─────────────┘     │  • JobExecutionService              │   │
│                      │  • WorkerAgentService               │   │
│                      │  • SchedulerService                 │   │
│                      │  • LogStreamService                 │   │
│                      └──────────────┬──────────────────────┘   │
│                                     │                          │
│                      ┌──────────────▼──────────────────────┐   │
│                      │         Worker Providers            │   │
│                      │  ┌────────┐ ┌────────┐ ┌──────────┐│   │
│                      │  │ Docker │ │  K8s   │ │Firecracker││   │
│                      │  └───┬────┘ └───┬────┘ └────┬─────┘│   │
│                      └──────┼──────────┼───────────┼──────┘   │
│                             │          │           │          │
│                      ┌──────▼──┐ ┌─────▼───┐ ┌─────▼─────┐   │
│                      │Container│ │   Pod   │ │  microVM  │   │
│                      │ Worker  │ │ Worker  │ │  Worker   │   │
│                      └─────────┘ └─────────┘ └───────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Flujo de un Job

1. **Usuario crea job** desde la web
2. **Job se encola** en PostgreSQL con estado `PENDING`
3. **Scheduler detecta** el job pendiente
4. **Provider aprovisiona** un worker (container/pod/microVM)
5. **Worker se registra** automáticamente con OTP
6. **Job se despacha** al worker
7. **Worker ejecuta** el comando y envía logs en tiempo real
8. **Job completa** y el resultado se guarda

### Estados del Job

| Estado      | Descripción                 |
| ----------- | --------------------------- |
| `PENDING`   | Esperando worker disponible |
| `ASSIGNED`  | Asignado a un worker        |
| `RUNNING`   | En ejecución                |
| `SUCCEEDED` | Completado exitosamente     |
| `FAILED`    | Terminó con error           |
| `CANCELLED` | Cancelado por el usuario    |
| `TIMEOUT`   | Excedió el tiempo límite    |

---

## 🔧 Troubleshooting

### La web no carga

```bash
# Verificar que los contenedores están corriendo
docker compose -f docker-compose.prod.yml ps

# Ver logs del frontend
docker compose -f docker-compose.prod.yml logs web

# Reiniciar servicios
docker compose -f docker-compose.prod.yml restart
```

### Los jobs quedan en PENDING

```bash
# Verificar que hay providers habilitados
docker compose -f docker-compose.prod.yml logs api | grep -i provider

# Verificar Docker socket
docker ps

# Verificar logs del servidor
docker compose -f docker-compose.prod.yml logs api
```

### Error de conexión a PostgreSQL

```bash
# Verificar que PostgreSQL está corriendo
docker compose -f docker-compose.prod.yml logs postgres

# Verificar conectividad
docker exec hodei-jobs-postgres pg_isready -U postgres
```

### Los logs no aparecen en tiempo real

- Verificar que el job está en estado `RUNNING`
- Refrescar la página de logs
- Verificar la conexión WebSocket en las herramientas de desarrollo del navegador

### Limpiar y reiniciar todo

```bash
# Parar y eliminar todo
docker compose -f docker-compose.prod.yml down -v

# Reiniciar desde cero
docker compose -f docker-compose.prod.yml up -d
```

---

## 👨‍💻 Referencia para Desarrolladores

Para información técnica detallada sobre:

- Compilación desde código fuente
- Tests unitarios y de integración
- API gRPC
- Desarrollo de nuevos providers

Consulta el archivo [DEVELOPMENT.md](./DEVELOPMENT.md) para la guía completa.

### Comandos útiles (Justfile)

Usamos `just` para automatizar tareas comunes. Ejecuta `just --list` para ver todos los comandos disponibles.

```bash
# Desarrollo
just dev            # Inicia todo el entorno
just dev-db         # Inicia solo la base de datos
just dev-backend    # Inicia backend con hot reload

# Testing
just test           # Ejecuta todos los tests
just test-backend   # Tests de backend
just test-e2e       # Tests end-to-end

# Calidad de Código
just check          # Lint y format check
just clean          # Limpiar artefactos
```

### Variables de Entorno

| Variable               | Descripción                            | Default |
| ---------------------- | -------------------------------------- | ------- |
| `HODEI_DATABASE_URL`   | URL de PostgreSQL                      | -       |
| `HODEI_DEV_MODE`       | Modo desarrollo (acepta tokens dev-\*) | `0`     |
| `HODEI_DOCKER_ENABLED` | Habilitar Docker provider              | `0`     |
| `HODEI_K8S_ENABLED`    | Habilitar Kubernetes provider          | `0`     |
| `HODEI_FC_ENABLED`     | Habilitar Firecracker provider         | `0`     |
| `GRPC_PORT`            | Puerto del servidor gRPC               | `50051` |
| `RUST_LOG`             | Nivel de logs                          | `info`  |

---

## 📚 Recursos Adicionales

- **README.md**: Descripción general del proyecto
- **README_ES.md**: README en español
- **docker-compose.prod.yml**: Configuración de producción
- **docker-compose.dev.yml**: Configuración de desarrollo

---

_¿Tienes preguntas? Abre un issue en el repositorio._
