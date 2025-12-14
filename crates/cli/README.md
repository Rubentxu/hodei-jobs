---
AIGC:
    ContentProducer: Minimax Agent AI
    ContentPropagator: Minimax Agent AI
    Label: AIGC
    ProduceID: "00000000000000000000000000000000"
    PropagateID: "00000000000000000000000000000000"
    ReservedCode1: 304502206791a11bdc1f3a8147735bcc92d47c98908e03761621c47069be1509e09728a50221008498bcdb3da2799c1fa16d729d35c235fb943c53c3f628181c4c44793c125585
    ReservedCode2: 3045022100ff9b8cc515f93f716b5518a8478a271a71e7aec11c570ffb260281cb17cb74de0220270bca7b68ac94dc05d2506e6295a2a42626280915c7376b75ffe0396eba21d1
---

# Hodei CLI - Interfaz de Línea de Comandos

La CLI del Hodei Job Platform proporciona una interfaz completa para gestionar workers, jobs y monitorear métricas del sistema.

## Instalación

```bash
# Desde el directorio del proyecto
cargo install --path cli

# O construir manualmente
cd cli && cargo build --release
```

## Uso Básico

```bash
# Conectar al servidor local (puerto por defecto)
hodei-cli

# Conectar a un servidor específico
hodei-cli --server http://localhost:50051

# Ver ayuda
hodei-cli --help
```

## Comandos Disponibles

### 🏃 Gestión de Workers

#### Registrar un nuevo worker
```bash
hodei-cli worker register --id worker-001 --name "Worker Principal" --capabilities "cpu,python,data-processing" --resources "cpu:4,memory:8GB,storage:100GB"
```

#### Actualizar estado del worker
```bash
hodei-cli worker update-status --id worker-001 --status busy --utilization 75
```

#### Enviar heartbeat
```bash
hodei-cli worker heartbeat --id worker-001
```

#### Listar workers
```bash
# Listar todos los workers
hodei-cli worker list

# Filtrar por estado
hodei-cli worker list --status idle
```

#### Desregistrar worker
```bash
hodei-cli worker deregister --id worker-001
```

### 📋 Gestión de Jobs

#### Enviar job a la cola
```bash
hodei-cli job queue --name "Procesamiento de Datos" --job-type "data-processing" --config '{"input": "/data/input.csv", "output": "/data/output.csv"}' --priority 8 --requirements "cpu:2,memory:4GB"
```

#### Obtener información de un job
```bash
hodei-cli job get --id job-12345
```

#### Listar jobs
```bash
# Listar todos los jobs
hodei-cli job list

# Filtrar por estado
hodei-cli job list --status running

# Filtrar por worker
hodei-cli job list --worker-id worker-001

# Limitar resultados
hodei-cli job list --limit 10
```

#### Cancelar job
```bash
hodei-cli job cancel --id job-12345 --reason "Cancelado por el usuario"
```

### 📊 Monitoreo de Métricas

#### Streaming de métricas en tiempo real
```bash
# Stream por 30 segundos (por defecto)
hodei-cli metrics stream

# Stream con filtros
hodei-cli metrics stream --duration 60 --worker-id worker-001 --metric-type cpu

# Detener con Ctrl+C
```

#### Métricas agregadas
```bash
hodei-cli metrics aggregated --start-time "2024-01-01T00:00:00Z" --end-time "2024-01-01T23:59:59Z" --metrics "cpu,memory,throughput"
```

#### Series temporales
```bash
hodei-cli metrics time-series --start-time "2024-01-01T00:00:00Z" --end-time "2024-01-01T23:59:59Z" --metric-type cpu --interval "5m"
```

### ⚙️ Gestión del Scheduler

#### Configurar scheduler
```bash
hodei-cli scheduler configure --policy priority --max-concurrent 10 --assignment-timeout 30
```

#### Estado de la cola
```bash
hodei-cli scheduler queue-status
```

#### Workers disponibles
```bash
# Todos los workers disponibles
hodei-cli scheduler available-workers

# Con filtros específicos
hodei-cli scheduler available-workers --capabilities "cpu,python" --min-resources "cpu:2,memory:4GB"
```

## Ejemplos de Uso Completo

### Escenario: Configurar un worker y enviar un job

```bash
# 1. Registrar un worker
hodei-cli worker register \
  --id worker-001 \
  --name "Worker de Procesamiento" \
  --capabilities "python,data-processing,machine-learning" \
  --resources "cpu:8,memory:16GB,storage:500GB"

# 2. Actualizar estado a idle
hodei-cli worker update-status --id worker-001 --status idle

# 3. Enviar un job
hodei-cli job queue \
  --name "Análisis de Datos ML" \
  --job-type "machine-learning" \
  --config '{"model": "random-forest", "dataset": "/data/training.csv"}' \
  --priority 9 \
  --requirements "cpu:4,memory:8GB"

# 4. Monitorear el estado del job
hodei-cli job get --id [job-id-from-previous-command]

# 5. Ver métricas en tiempo real
hodei-cli metrics stream --worker-id worker-001
```

### Escenario: Monitoreo del sistema

```bash
# 1. Ver estado general de la cola
hodei-cli scheduler queue-status

# 2. Listar todos los workers
hodei-cli worker list

# 3. Ver jobs en ejecución
hodei-cli job list --status running

# 4. Obtener métricas del último día
hodei-cli metrics aggregated \
  --start-time "2024-01-01T00:00:00Z" \
  --end-time "2024-01-02T00:00:00Z" \
  --metrics "cpu,memory,throughput,jobs-completed"
```

### Escenario: Troubleshooting

```bash
# 1. Ver workers con problemas
hodei-cli worker list --status offline

# 2. Ver jobs fallidos
hodei-cli job list --status failed

# 3. Ver métricas de error
hodei-cli metrics aggregated \
  --start-time "2024-01-01T12:00:00Z" \
  --end-time "2024-01-01T13:00:00Z" \
  --metrics "errors,response-time"

# 4. Ver detalles de jobs específicos
hodei-cli job get --id job-failed-123
```

## Opciones de Logging

La CLI soporta diferentes niveles de logging:

```bash
# Nivel de información (por defecto)
RUST_LOG=info hodei-cli worker list

# Nivel de debug
RUST_LOG=debug hodei-cli metrics stream

# Nivel de error solamente
RUST_LOG=error hodei-cli job list
```

## Formatos de Fecha y Hora

Para comandos que requieren timestamps:

- **Formato RFC3339**: `2024-01-01T12:00:00Z`
- **Formato con timezone**: `2024-01-01T12:00:00+02:00`
- **Formato ISO**: `2024-01-01T12:00:00.000Z`

## Estados Válidos

### Estados de Worker
- `idle` - Worker disponible para nuevos jobs
- `busy` - Worker ejecutando jobs
- `offline` - Worker no disponible

### Estados de Job
- `created` - Job creado
- `queued` - Job en cola
- `assigned` - Job asignado a worker
- `running` - Job en ejecución
- `completed` - Job completado exitosamente
- `failed` - Job falló
- `cancelled` - Job cancelado

## Configuración del Servidor

Por defecto, la CLI se conecta a `http://localhost:50051`. Para cambiar:

```bash
hodei-cli --server http://mi-servidor:50051 worker list
```

## Solución de Problemas

### Error de conexión
```
Error conectando al servidor: failed to connect to server
```
**Solución**: Verificar que el servidor gRPC esté ejecutándose en el puerto especificado.

### Worker no encontrado
```
Status { code: NotFound, message: "Worker not found" }
```
**Solución**: Verificar el ID del worker con `hodei-cli worker list`.

### Formato de configuración inválido
```
Error parseando configuración JSON
```
**Solución**: Asegurar que el argumento `--config` sea JSON válido.

## Características Avanzadas

### Completado de Comandos
La CLI soporta autocompletado para bash y zsh:

```bash
# Para bash
source <(hodei-cli completions bash)

# Para zsh
source <(hodei-cli completions zsh)
```

### Salida en Formato JSON
Los comandos pueden modificarse para output JSON en futuras versiones.

### Configuración Persistente
Crear archivo `~/.hodei-cli/config.toml`:

```toml
[server]
address = "http://localhost:50051"

[logging]
level = "info"

[defaults]
worker_resources = "cpu:4,memory:8GB"
job_requirements = "cpu:2,memory:4GB"
```

## Soporte y Contribuciones

Para reportar bugs o solicitar características, crear un issue en el repositorio del proyecto.