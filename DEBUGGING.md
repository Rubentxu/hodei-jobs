# 🚨 Guía Completa de Debug y Monitoreo - Hodei Jobs Platform

## 📋 Índice
1. [Configuración del Entorno de Debug](#configuración-del-entorno-de-debug)
2. [Flujo de Debug Estructurado](#flujo-de-debug-estructurado)
3. [Herramientas de Diagnóstico](#herramientas-de-diagnóstico)
4. [Consultas SQL Esenciales](#consultas-sql-esenciales)
5. [Scripts de Utilidad](#scripts-de-utilidad)
6. [Escenarios de Fallo Comunes](#escenarios-de-fallo-comunes)
7. [Estrategias de Logging](#estrategias-de-logging)
8. [Comandos Just para Debug](#comandos-just-para-debug)

---

## 1. Configuración del Entorno de Debug

### 🔧 Configuración de Logging

**Servidor (`crates/server/bin/src/main.rs`):**
```rust
// Asegúrate de que el logging esté configurado en INFO o DEBUG
let subscriber = FmtSubscriber::builder()
    .with_max_level(Level::INFO)  // Cambiar a DEBUG para más detalle
    .with_env_filter(EnvFilter::from_default_env())
    .with_target(true)
    .finish();
```

**Variables de Entorno Recomendadas:**
```bash
# En servidor
export RUST_LOG=hodei_server_application=DEBUG,hodei_server_interface=DEBUG
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/postgres"

# En worker
export HODEI_OTP_TOKEN=<token>
export HODEI_SERVER_ADDRESS=localhost:50051
export RUST_LOG=hodei_worker_application=DEBUG
```

### 🖥️ Setup de Terminal Multipane (Recomendado)

**Pane 1 - Logs del Servidor:**
```bash
tail -f /tmp/server.log | grep -E "JobDispatcher|JobController|JobCreated|JobAssigned|RUN_JOB"
```

**Pane 2 - Estado de Base de Datos:**
```bash
watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c \"SELECT id, state, attempts, created_at FROM jobs ORDER BY created_at DESC LIMIT 5;\""
```

**Pane 3 - Workers Registrados:**
```bash
watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c \"SELECT id, state, last_heartbeat FROM workers;\""
```

**Pane 4 - Cola de Jobs:**
```bash
watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c \"SELECT COUNT(*) as queued_jobs FROM job_queue;\""
```

---

## 2. Flujo de Debug Estructurado

### 🎯 Metodología de 5 Pasos

Cuando un job falle o se quede colgado, sigue este flujo:

#### **Paso 1: Capturar el Job ID**
```bash
# De la salida del CLI o logs
JOB_ID="584a465b-d208-4a05-beef-8671b9bc2805"
```

#### **Paso 2: Verificar Creación del Job**
```bash
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT id, state, spec->'command' as command, created_at
FROM jobs
WHERE id = '$JOB_ID';
"
```

**Estado Esperado:** `PENDING`

#### **Paso 3: Verificar Encolado**
```bash
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT jq.job_id, jq.enqueued_at, j.state
FROM job_queue jq
JOIN jobs j ON jq.job_id = j.id
WHERE jq.job_id = '$JOB_ID';
"
```

**Estado Esperado:** Job debe aparecer en `job_queue`

#### **Paso 4: Verificar Workers Disponibles**
```bash
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT id, state, last_heartbeat,
       EXTRACT(EPOCH FROM (now() - last_heartbeat)) as seconds_since_heartbeat
FROM workers
WHERE state = 'READY' AND last_heartbeat > now() - interval '30 seconds';
"
```

**Estado Esperado:** Al menos 1 worker READY y con heartbeat reciente

#### **Paso 5: Rastrear Eventos**
```bash
# Si existe tabla de eventos o audit_logs
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT occurred_at, event_type, payload
FROM job_events
WHERE job_id = '$JOB_ID'
ORDER BY occurred_at ASC;
"
```

---

## 3. Herramientas de Diagnóstico

### 🔍 Script de Diagnóstico Completo (`debug-job.sh`)

```bash
#!/bin/bash
# Guardar como: scripts/debug-job.sh

if [ -z "$1" ]; then
    echo "Uso: $0 <JOB_ID>"
    exit 1
fi

JOB_ID=$1

echo "=================================================="
echo "  DIAGNÓSTICO COMPLETO DE JOB: $JOB_ID"
echo "=================================================="
echo ""

echo "1. ESTADO DEL JOB:"
echo "------------------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT 'ID: ' || id || ' | Estado: ' || state || ' | Intentos: ' || attempts
FROM jobs WHERE id = '$JOB_ID';
"

echo ""
echo "2. EN COLA:"
echo "-----------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT CASE WHEN EXISTS (SELECT 1 FROM job_queue WHERE job_id = '$JOB_ID')
            THEN 'SÍ - Está en job_queue'
            ELSE 'NO - No está en job_queue'
       END;
"

echo ""
echo "3. WORKERS DISPONIBLES:"
echo "-----------------------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT COUNT(*) as workers_ready
FROM workers
WHERE state = 'READY' AND last_heartbeat > now() - interval '30 seconds';
"

echo ""
echo "4. ESPECIFICACIÓN DEL JOB:"
echo "--------------------------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT jsonb_pretty(spec) FROM jobs WHERE id = '$JOB_ID';
"

echo ""
echo "5. WORKERS RECIENTES:"
echo "---------------------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT id, state, last_heartbeat
FROM workers
WHERE last_heartbeat > now() - interval '2 minutes'
ORDER BY last_heartbeat DESC
LIMIT 3;
"

echo ""
echo "=================================================="
```

### 📊 Dashboard de Estado del Sistema (`system-status.sh`)

```bash
#!/bin/bash
# Guardar como: scripts/system-status.sh

echo "╔════════════════════════════════════════════════════════════╗"
echo "║           ESTADO DEL SISTEMA HODEI JOBS                    ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

echo "📊 JOBS:"
echo "--------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT
  SUM(CASE WHEN state = 'PENDING' THEN 1 ELSE 0 END) as PENDING,
  SUM(CASE WHEN state = 'RUNNING' THEN 1 ELSE 0 END) as RUNNING,
  SUM(CASE WHEN state = 'SUCCEEDED' THEN 1 ELSE 0 END) as SUCCEEDED,
  SUM(CASE WHEN state = 'FAILED' THEN 1 ELSE 0 END) as FAILED
FROM jobs;
"

echo ""
echo "👥 WORKERS:"
echo "-----------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT
  state,
  COUNT(*) as count
FROM workers
GROUP BY state;
"

echo ""
echo "📋 COLA DE JOBS:"
echo "----------------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT COUNT(*) as jobs_en_cola FROM job_queue;
"

echo ""
echo "⚙️ PROVIDERS:"
echo "-------------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT name, status, provider_type FROM provider_configs;
"

echo ""
echo "🔑 TOKENS ACTIVOS:"
echo "------------------"
docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT
  COUNT(*) as total_tokens,
  COUNT(CASE WHEN consumed_at IS NULL AND expires_at > now() THEN 1 END) as tokens_disponibles
FROM worker_bootstrap_tokens;
"
```

---

## 4. Consultas SQL Esenciales

### 🔎 Ver Jobs Recientes con Detalles

```sql
SELECT
    j.id,
    j.state,
    j.attempts,
    j.created_at,
    jq.enqueued_at,
    CASE
        WHEN jq.job_id IS NOT NULL THEN 'En Cola'
        ELSE 'No En Cola'
    END as queue_status
FROM jobs j
LEFT JOIN job_queue jq ON j.id = jq.job_id
ORDER BY j.created_at DESC
LIMIT 10;
```

### 📈 Workers con Heartbeat Reciente

```sql
SELECT
    id,
    state,
    last_heartbeat,
    EXTRACT(EPOCH FROM (now() - last_heartbeat)) as seconds_ago,
    current_job_id
FROM workers
ORDER BY last_heartbeat DESC;
```

### 🎯 Jobs que Nunca Salieron de PENDING

```sql
SELECT
    j.id,
    j.state,
    j.created_at,
    EXTRACT(EPOCH FROM (now() - j.created_at)) as seconds_since_creation
FROM jobs j
LEFT JOIN job_queue jq ON j.id = jq.job_id
WHERE j.state = 'PENDING'
  AND jq.job_id IS NULL
ORDER BY j.created_at ASC;
```

### 📡 Secuencia de Eventos de un Job

```sql
SELECT
    occurred_at,
    event_type,
    payload
FROM job_events
WHERE job_id = 'TU_JOB_ID_AQUI'
ORDER BY occurred_at ASC;
```

---

## 4.1. 🎯 **NUEVO: Sistema de Auditoría con Domain Events**

El sistema ahora incluye un **sistema completo de auditoría** que registra todos los eventos de dominio en la tabla `domain_events`.

### ✨ **Características Principales**

✅ **Correlation ID**: Rastrea jobs relacionados entre sí  
✅ **Actor**: Identifica quién inició la acción  
✅ **Timeline Persistente**: Eventos guardados en base de datos  
✅ **Script de Diagnóstico**: `debug-job-timeline.sh`  

### 🔍 **Timeline Completo de un Job**

```bash
# Usar el script de diagnóstico
just debug-job-timeline <JOB_ID>

# O consulta SQL directa
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    to_char(occurred_at, 'HH24:MI:SS.MS') as time,
    event_type,
    substring(aggregate_id from 1 for 8) as entity_id,
    COALESCE(actor, 'unknown') as actor,
    COALESCE(substring(correlation_id from 1 for 12), '-') as corr_id,
    left(payload::text, 60) || '...' as summary
FROM domain_events
WHERE correlation_id = '<JOB_ID>' OR aggregate_id = '<JOB_ID>'
ORDER BY occurred_at ASC;
"
```

**Ejemplo de Salida:**
```
🔍 Fetching timeline for Job ID: 2cf98dd9-d8cd-4b22-870b-d9b17fe4a5ea
────────────────────────────────────────
     time     |      event       | entity_id |         actor         | corr_id  | summary
--------------+------------------+-----------+-----------------------+----------+--------
 12:58:18.459 | JobCreated       | 2cf98dd9  | unknown               | -        | ...
 12:58:18.640 | JobAssigned      | 2cf98dd9  | system:job_dispatcher | 2cf98dd9 | ...
 12:58:18.860 | JobStatusChanged | 2cf98dd9  | unknown               | -        | ...
 12:58:28.728 | JobStatusChanged | 2cf98dd9  | unknown               | -        | ...
(4 rows)
```

### 🎯 **Consultas SQL para Auditoría**

#### **1. Todos los eventos de un Job (por correlation_id)**
```sql
SELECT
    to_char(occurred_at, 'HH24:MI:SS') as time,
    event_type,
    actor,
    payload->>'job_id' as job_id,
    payload->>'worker_id' as worker_id
FROM domain_events
WHERE correlation_id = '<JOB_ID>'
ORDER BY occurred_at ASC;
```

#### **2. Jobs por workflow (mismo correlation_id)**
```sql
SELECT
    aggregate_id,
    event_type,
    occurred_at,
    actor
FROM domain_events
WHERE correlation_id = 'workflow-123'
ORDER BY aggregate_id, occurred_at;
```

#### **3. Eventos por Actor**
```sql
SELECT
    actor,
    COUNT(*) as event_count,
    MIN(occurred_at) as first_event,
    MAX(occurred_at) as last_event
FROM domain_events
WHERE occurred_at > now() - interval '1 hour'
GROUP BY actor
ORDER BY event_count DESC;
```

#### **4. Detectar Jobs Sin Auditoría**
```sql
SELECT
    j.id,
    j.state,
    j.created_at,
    CASE
        WHEN de.aggregate_id IS NULL THEN '⚠️ Sin eventos'
        ELSE '✅ Con eventos'
    END as audit_status
FROM jobs j
LEFT JOIN domain_events de ON de.aggregate_id = j.id::text
WHERE j.created_at > now() - interval '1 hour'
ORDER BY j.created_at DESC;
```

### 🏷️ **Uso de Correlation ID**

Cuando creas un job, puedes especificar un correlation_id para rastrear un workflow completo:

```bash
# Crear job con correlation_id
cargo run --bin hodei-jobs-cli -- job run \
  --name "ETL Pipeline" \
  --correlation-id "workflow-2025-12-22" \
  --command "echo 'Step 1'"

# Ver todo el workflow
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    aggregate_id,
    event_type,
    to_char(occurred_at, 'HH24:MI:SS') as time,
    payload->>'job_id' as job_id
FROM domain_events
WHERE correlation_id = 'workflow-2025-12-22'
ORDER BY occurred_at;
"
```

### 📊 **Estadísticas de Auditoría**

```sql
-- Eventos por tipo en la última hora
SELECT
    event_type,
    COUNT(*) as count,
    COUNT(DISTINCT correlation_id) as unique_workflows
FROM domain_events
WHERE occurred_at > now() - interval '1 hour'
GROUP BY event_type
ORDER BY count DESC;

-- Actores más activos
SELECT
    actor,
    COUNT(*) as events,
    COUNT(DISTINCT correlation_id) as workflows
FROM domain_events
WHERE occurred_at > now() - interval '1 day'
  AND actor IS NOT NULL
GROUP BY actor
ORDER BY events DESC
LIMIT 10;
```

### ⚠️ **Detectar Problemas con Auditoría**

```sql
-- Jobs que cambiaron de estado sin eventos intermedios
SELECT DISTINCT
    j1.id,
    j1.state as final_state,
    j1.created_at
FROM jobs j1
WHERE NOT EXISTS (
    SELECT 1 FROM domain_events de
    WHERE de.aggregate_id = j1.id::text
      AND de.event_type = 'JobStatusChanged'
)
  AND j1.state != 'PENDING';

-- Workers con eventos de desconexión
SELECT
    worker_id,
    to_char(occurred_at, 'HH24:MI:SS') as time,
    payload->>'last_heartbeat' as last_heartbeat
FROM domain_events
WHERE event_type = 'WorkerStatusChanged'
  AND payload->>'new_status' = 'Terminated'
  AND occurred_at > now() - interval '1 hour'
ORDER BY occurred_at DESC;
```

---

## 5. Scripts de Utilidad

### 🧹 Limpiar Sistema Completamente (`clean-system.sh`)

```bash
#!/bin/bash
# Guardar como: scripts/clean-system.sh

echo "🧹 Limpiando sistema Hodei..."

# 1. Matar todos los workers
echo "  - Matando workers..."
killall -9 hodei-worker-bin 2>/dev/null || true

# 2. Matar servidor
echo "  - Matando servidor..."
killall -9 hodei-server-bin 2>/dev/null || true

# 3. Limpiar contenedores Docker antiguos
echo "  - Limpiando contenedores Docker..."
docker ps -a | grep hodei-worker | awk '{print $1}' | xargs -r docker rm -f 2>/dev/null || true

# 4. Limpiar base de datos
echo "  - Limpiando base de datos..."
docker exec hodei-jobs-postgres psql -U postgres -c "
DROP TABLE IF EXISTS job_queue CASCADE;
DROP TABLE IF EXISTS jobs CASCADE;
DROP TABLE IF EXISTS workers CASCADE;
DROP TABLE IF EXISTS worker_bootstrap_tokens CASCADE;
DROP TABLE IF EXISTS job_events CASCADE;
" 2>/dev/null || true

echo "✅ Sistema limpiado"
```

### 🚀 Reinicio Completo del Sistema (`restart-system.sh`)

```bash
#!/bin/bash
# Guardar como: scripts/restart-system.sh

./scripts/clean-system.sh
sleep 2

echo "🚀 Reiniciando sistema..."

# 1. Levantar PostgreSQL si no está corriendo
if ! docker ps | grep -q hodei-jobs-postgres; then
    echo "  - Levantando PostgreSQL..."
    docker run -d --name hodei-jobs-postgres \
        -e POSTGRES_PASSWORD=postgres \
        -e POSTGRES_USER=postgres \
        -e POSTGRES_DB=postgres \
        -p 5432:5432 \
        postgres:16-alpine
    sleep 5
fi

# 2. Configurar provider Docker
echo "  - Configurando provider Docker..."
docker exec hodei-jobs-postgres psql -U postgres -c "
INSERT INTO provider_configs (id, name, provider_type, config, status, priority, max_workers, tags, metadata)
VALUES (
  'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
  'Docker',
  'Docker',
  '{\"socket_path\": \"/var/run/docker.sock\"}',
  'ACTIVE',
  0,
  10,
  '[]'::jsonb,
  '{}'::jsonb
) ON CONFLICT (id) DO UPDATE SET
  name = EXCLUDED.name,
  config = EXCLUDED.config,
  status = EXCLUDED.status,
  updated_at = now();
" 2>/dev/null || true

# 3. Crear bootstrap token
echo "  - Creando bootstrap token..."
TOKEN=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "
INSERT INTO worker_bootstrap_tokens (token, worker_id, expires_at)
VALUES (gen_random_uuid(), '00000000-0000-0000-0000-000000000000', now() + interval '1 hour')
RETURNING token;
" | xargs)

# 4. Levantar servidor
echo "  - Levantando servidor..."
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/postgres"
cargo run --bin hodei-server-bin > /tmp/server.log 2>&1 &
SERVER_PID=$!
echo "    Servidor PID: $SERVER_PID"
sleep 8

# 5. Levantar worker
echo "  - Levantando worker..."
HODEI_OTP_TOKEN=$TOKEN HODEI_SERVER_ADDRESS=localhost:50051 \
    cargo run --bin hodei-worker-bin > /tmp/worker.log 2>&1 &
WORKER_PID=$!
echo "    Worker PID: $WORKER_PID"

echo ""
echo "✅ Sistema reiniciado"
echo "   Token: $TOKEN"
echo "   Server PID: $SERVER_PID"
echo "   Worker PID: $WORKER_PID"
```

---

## 6. Escenarios de Fallo Comunes

### ❌ **Escenario 1: Job se Crea pero Nunca Llega al Worker**

**Síntomas:**
- CLI dice "Job queued successfully"
- Job aparece en `jobs` como `PENDING`
- No hay logs de "RUN_JOB" en el servidor
- Worker no recibe jobs

**Diagnóstico:**
```bash
# 1. Verificar workers disponibles
./scripts/system-status.sh

# 2. Ver logs del servidor
tail -100 /tmp/server.log | grep -E "No jobs in queue|No available workers"

# 3. Verificar workers en DB
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT id, state, last_heartbeat
FROM workers;
"
```

**Causas Probables:**
1. **No hay workers registrados** → Solución: Verificar token OTP y conexión worker
2. **Workers sin heartbeat reciente** → Solución: Worker desconectado o colgado
3. **Dispatcher no está corriendo** → Solución: Verificar que `JobController` esté activo

**Parche:**
```bash
# Reiniciar worker con token correcto
HODEI_OTP_TOKEN=<TOKEN> HODEI_SERVER_ADDRESS=localhost:50051 \
    cargo run --bin hodei-worker-bin 2>&1 | tee /tmp/worker.log &
```

---

### ❌ **Escenario 2: Worker No Se Puede Registrar**

**Síntomas:**
- Logs del worker: "Invalid OTP token: OTP token must be a UUID"
- Worker aparece como "REGISTERING" o no aparece en DB
- Jobs se encolan pero no hay workers para procesarlos

**Diagnóstico:**
```bash
# 1. Verificar que el token existe
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT token, expires_at, consumed_at
FROM worker_bootstrap_tokens
WHERE expires_at > now() AND consumed_at IS NULL;
"

# 2. Verificar que el token se pasó correctamente al worker
grep "HODEI_OTP_TOKEN" /tmp/worker.log || echo "⚠ Token no configurado"
```

**Causas Probables:**
1. **Token incorrecto** → Solución: Crear nuevo token con `gen_random_uuid()`
2. **Token expirado** → Solución: Token con `expires_at < now()`
3. **Token ya usado** → Solución: Token con `consumed_at != NULL`
4. **Variable de entorno incorrecta** → Solución: Usar `HODEI_OTP_TOKEN` no `BOOTSTRAP_TOKEN`

**Parche:**
```bash
# Crear nuevo token
TOKEN=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "
INSERT INTO worker_bootstrap_tokens (token, worker_id, expires_at)
VALUES (gen_random_uuid(), '00000000-0000-0000-0000-000000000000', now() + interval '1 hour')
RETURNING token;
" | xargs)

# Reiniciar worker
HODEI_OTP_TOKEN=$TOKEN HODEI_SERVER_ADDRESS=localhost:50051 \
    cargo run --bin hodei-worker-bin 2>&1 | tee /tmp/worker.log &
```

---

### ❌ **Escenario 3: Job se Envía al Worker pero No Hay Logs**

**Síntomas:**
- Servidor dice "RUN_JOB command sent successfully"
- Worker recibe el job
- Job se queda en estado RUNNING pero no hay logs en CLI
- CLI se queda esperando logs infinitamente

**Diagnóstico:**
```bash
# 1. Verificar que el job está RUNNING
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT id, state, started_at
FROM jobs
WHERE id = 'TU_JOB_ID';
"

# 2. Verificar logs del worker
tail -100 /tmp/worker.log | grep -E "Received job|Executing|command"
```

**Causas Probables:**
1. **LogBatcher no flush** → Solución: Timeout en LogBatcher
2. **gRPC stream cerrado** → Solución: Verificar `LogStreamService`
3. **Worker no ejecuta el script** → Solución: Verificar permisos y path

**Parche:**
Verificar en `/home/rubentxu/Proyectos/rust/package/hodei-job-platform/crates/worker/infrastructure/src/logging.rs`:

```rust
pub async fn flush(&mut self) -> bool {
    if self.buffer.is_empty() {
        return true;
    }
    let batch = std::mem::take(&mut self.buffer);
    let msg = WorkerMessage {
        payload: Some(WorkerPayload::LogBatch(hodei_jobs::LogBatch {
            job_id: batch[0].job_id.clone(),
            entries: batch,
        })),
    };

    // ✅ Usar timeout para evitar bloqueo
    match tokio::time::timeout(Duration::from_secs(5), self.tx.send(msg)).await {
        Ok(result) => match result {
            Ok(_) => {
                self.last_flush = Instant::now();
                true
            }
            Err(_) => {
                warn!("Failed to send log batch to server");
                false
            }
        },
        Err(_) => {
            warn!("Log batch send timed out after 5 seconds");
            false
        }
    }
}
```

---

### ❌ **Escenario 4: Jobs Cambian a SCHEDULED Automáticamente**

**Síntomas:**
- Job creado como PENDING
- Inmediatamente cambia a SCHEDULED sin confirmación del worker
- Flujo incoherente

**Diagnóstico:**
```bash
# Ver el estado antes y después
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT id, state, updated_at
FROM jobs
WHERE id = 'TU_JOB_ID';
"
```

**Causas Probables:**
1. **Dispatcher publica JobStatusChanged prematuro** → Solución: Eliminar evento `PENDING → SCHEDULED`
2. **submit_to_provider cambia estado** → Solución: Usar `assign_to_provider` en su lugar

**Parche Implementado:**
En `/home/rubentxu/Proyectos/rust/package/hodei-job-platform/crates/server/application/src/jobs/dispatcher.rs`:

```rust
// ✅ ANTES (Problemático):
job.submit_to_provider(provider_id.clone(), context.clone())?;
// Esto cambia el estado a SCHEDULED inmediatamente

// ✅ DESPUÉS (Correcto):
job.assign_to_provider(provider_id.clone(), context)?;
// Esto mantiene el estado como PENDING hasta que worker confirme
```

**Flujo Correcto:**
```
1. Job creado ──→ PENDING
2. JobDispatcher dequeuea ──→ PENDING
3. JobDispatcher asigna ──→ PENDING (sin cambiar estado)
4. JobDispatcher envía RUN_JOB ──→ PENDING
5. Worker recibe y envía ACK ──→ [confirma ejecución]
6. Server actualiza ──→ RUNNING (basado en mensaje del worker)
7. Server publica evento ──→ RUNNING
```

---

---

## 7. Estrategias de Logging

### 🏷️ **Uso de Correlation ID**

Cada operación debe tener un `correlation_id` único para rastrear su flujo completo:

```rust
// En el CLI
let correlation_id = format!("CLI-{}", uuid::Uuid::new_v4());

// En CreateJobUseCase
let event = DomainEvent::JobCreated {
    job_id: job_id.clone(),
    spec: job_spec,
    occurred_at: Utc::now(),
    correlation_id: Some(correlation_id.clone()),  // ← Incluir correlation_id
    actor: Some("cli".to_string()),
};

// En todos los logs
tracing::info!(
    correlation_id = %correlation_id,
    job_id = %job_id,
    "Job created successfully"
);
```

### 📝 **Logs Estructurados**

Usa `tracing` con campos estructurados:

```rust
// ❌ Malo:
info!("Job {} dispatched to worker {}", job_id, worker_id);

// ✅ Bueno:
info!(
    job_id = %job_id,
    worker_id = %worker_id,
    state = ?job.state(),
    "Dispatching job to worker"
);
```

### 🔍 **Filtros de Log Útiles**

```bash
# Ver solo logs de un job específico
grep "584a465b-d208-4a05-beef-8671b9bc2805" /tmp/server.log

# Ver solo logs del dispatcher
grep "JobDispatcher" /tmp/server.log

# Ver solo eventos de cambio de estado
grep "JobStatusChanged" /tmp/server.log

# Ver logs de un worker específico
grep "worker-abc123" /tmp/worker.log
```

---

## 8. Comandos Just para Debug

Añadir a `justfile`:

```makefile
# Debug jobs
debug-jobs:
    @echo "=== JOBS EN ESTADO PENDING ==="
    docker exec hodei-jobs-postgres psql -U postgres -c "SELECT id, state, created_at FROM jobs WHERE state = 'PENDING' ORDER BY created_at DESC;"

debug-queue:
    @echo "=== JOBS EN COLA ==="
    docker exec hodei-jobs-postgres psql -U postgres -c "SELECT jq.job_id, jq.enqueued_at, j.state FROM job_queue jq JOIN jobs j ON jq.job_id = j.id;"

debug-workers:
    @echo "=== WORKERS ==="
    docker exec hodei-jobs-postgres psql -U postgres -c "SELECT id, state, last_heartbeat, EXTRACT(EPOCH FROM (now() - last_heartbeat)) as seconds_ago FROM workers;"

debug-system:
    @./scripts/system-status.sh

debug-job:
    @read -p "Job ID: " JOB_ID; \
    ./scripts/debug-job.sh $$JOB_ID

# Limpieza
clean-all:
    @./scripts/clean-system.sh

restart-all:
    @./scripts/restart-system.sh

# Monitoreo
watch-jobs:
    @watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c 'SELECT id, state, created_at FROM jobs ORDER BY created_at DESC LIMIT 5;'"

watch-workers:
    @watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c 'SELECT id, state, last_heartbeat FROM workers;'"

watch-queue:
    @watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c 'SELECT COUNT(*) as queued FROM job_queue;'"

# Logs
logs-server:
    @tail -f /tmp/server.log | grep -E "JobDispatcher|JobController|JobCreated|RUN_JOB"

logs-worker:
    @tail -f /tmp/worker.log | grep -E "Received job|Executing|LogBatch"
```

---

## ✅ Checklist de Debug Rápido

Cuando algo falle:

- [ ] **Capturar Job ID** de la salida del CLI
- [ ] **Verificar estado en DB** con `debug-job <JOB_ID>`
- [ ] **Verificar workers** con `debug-workers`
- [ ] **Ver logs del servidor** con `logs-server`
- [ ] **Ver logs del worker** con `logs-worker`
- [ ] **Si no hay workers:** Verificar token OTP y reiniciar worker
- [ ] **Si job no avanza:** Ver si está en cola y hay workers disponibles
- [ ] **Si no hay logs:** Verificar LogBatcher y gRPC stream
- [ ] **Reiniciar todo** con `restart-all` si es necesario

---

## 📚 Referencias

- **Documentación de `tracing`:** https://docs.rs/tracing
- **Patrones de logging estructurado:** https://crates.io/crates/tracing
- **PostgreSQL LISTEN/NOTIFY:** https://www.postgresql.org/docs/current/sql-listen.html
- **gRPC en Rust (tonic):** https://docs.rs/tonic

---

## 🎯 Resumen de Cambios Realizados

### ✅ **Problema 1: Jobs en Estado SCHEDULED Prematuro**

**Causa:** `dispatcher.rs` publicaba evento `JobStatusChanged (PENDING → SCHEDULED)` inmediatamente después de enviar RUN_JOB.

**Solución:** Eliminado el evento prematuro y uso de `assign_to_provider()` que mantiene PENDING hasta confirmación del worker.

**Archivos Modificados:**
- `crates/server/domain/src/jobs/aggregate.rs` - Nuevo método `assign_to_provider()`
- `crates/server/application/src/jobs/dispatcher.rs` - Eliminado evento prematuro

### ✅ **Problema 2: Worker No Recibe Jobs**

**Causa:** Estado SCHEDULED no estaba en la query del dispatcher.

**Solución:** El dispatcher ahora mantiene jobs en PENDING hasta que worker confirme con acknowledgment.

### ✅ **Problema 3: Logs No Llegan al CLI**

**Causa:** LogBatcher usaba `try_send()` que falla cuando el buffer está lleno.

**Solución:** Cambiado a `tokio::time::timeout()` con timeout de 5 segundos.

**Archivo Modificado:**
- `crates/worker/infrastructure/src/logging.rs` - `flush()` con timeout

---

## 🔍 **Búsqueda de Errores con Logs Estructurados y Eventos**

### 🎯 **Estrategia de Debug Basada en Auditoría**

El sistema de auditoría con `EventMetadata` permite rastrear flujos completos de jobs usando `correlation_id` y `actor`. Esto es fundamental para encontrar errores y mantener coherencia secuencial.

### 📝 **Logs Estructurados con Correlation ID**

Todos los logs del servidor incluyen ahora correlation_id y actor:

```rust
// Ejemplo de log estructurado
info!(
    job_id = %job.id,
    correlation_id = %metadata.correlation_id,
    actor = %metadata.actor,
    "Job dispatched successfully"
);
```

**Filtros de Log Útiles:**
```bash
# Ver logs de un job específico (por correlation_id)
tail -f /tmp/server.log | grep -E "correlation_id.*2cf98dd9"

# Ver logs de un actor específico
tail -f /tmp/server.log | grep -E "actor.*system:job_dispatcher"

# Ver logs de un workflow completo
tail -f /tmp/server.log | grep -E "correlation_id.*workflow-2025"
```

### 🎯 **Verificar Coherencia Secuencial de Eventos**

#### **Paso 1: Obtener Timeline Completo**
```bash
# Usar el script de timeline
just debug-job-timeline <JOB_ID>

# Verificar secuencia esperada:
# 1. JobCreated (actor: usuario/sistema)
# 2. JobAssigned (actor: system:job_dispatcher)
# 3. JobStatusChanged (PENDING → RUNNING)
# 4. JobStatusChanged (RUNNING → SUCCEEDED/FAILED)
```

#### **Paso 2: Validar Transiciones de Estado**
```sql
-- Verificar que no hay transiciones inválidas
SELECT
    aggregate_id,
    event_type,
    to_char(occurred_at, 'HH24:MI:SS.MS') as time,
    actor,
    -- Extraer estados del payload
    payload->>'old_state' as old_state,
    payload->>'new_state' as new_state
FROM domain_events
WHERE event_type = 'JobStatusChanged'
  AND (aggregate_id = '<JOB_ID>' OR correlation_id = '<JOB_ID>')
ORDER BY occurred_at;
```

#### **Paso 3: Detectar Secuencias Inconsistentes**

```sql
-- Jobs con eventos fuera de orden (ej: RUNNING antes de ASSIGNED)
WITH event_sequence AS (
    SELECT
        aggregate_id,
        event_type,
        occurred_at,
        ROW_NUMBER() OVER (PARTITION BY aggregate_id ORDER BY occurred_at) as event_order
    FROM domain_events
    WHERE aggregate_id = '<JOB_ID>'
)
SELECT
    es1.aggregate_id,
    es1.event_type as event_1,
    es2.event_type as event_2,
    es1.occurred_at as time_1,
    es2.occurred_at as time_2
FROM event_sequence es1
JOIN event_sequence es2 ON es1.aggregate_id = es2.aggregate_id
WHERE es1.event_order + 1 = es2.event_order
  AND es1.event_type = 'JobStatusChanged'
  AND es2.event_type = 'JobAssigned';
```

### 🔍 **Casos de Uso Prácticos**

#### **Caso 1: Job Desapareció en el Pipeline**
```bash
# 1. Obtener correlation_id del job
JOB_ID="2cf98dd9-d8cd-4b22-870b-d9b17fe4a5ea"

# 2. Ver timeline completo
just debug-job-timeline $JOB_ID

# 3. Ver logs con correlation_id
tail -f /tmp/server.log | grep -E "correlation_id.*$JOB_ID"

# 4. Verificar si hay eventos faltantes
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT event_type, COUNT(*)
FROM domain_events
WHERE correlation_id = '$JOB_ID'
GROUP BY event_type;
"
```

#### **Caso 2: Worker No Procesa Jobs**
```sql
-- Ver si JobAssigned llega al worker
SELECT
    to_char(occurred_at, 'HH24:MI:SS.MS') as time,
    event_type,
    payload->>'worker_id' as worker_id,
    payload->>'job_id' as job_id
FROM domain_events
WHERE event_type = 'JobAssigned'
  AND occurred_at > now() - interval '10 minutes'
ORDER BY occurred_at DESC;

-- Verificar si worker envió acknowledgment
SELECT
    to_char(occurred_at, 'HH24:MI:SS.MS') as time,
    event_type,
    payload->>'job_id' as job_id,
    payload->>'success' as success
FROM domain_events
WHERE event_type = 'JobStatusChanged'
  AND payload->>'new_state' = 'RUNNING'
  AND occurred_at > now() - interval '10 minutes'
ORDER BY occurred_at DESC;
```

#### **Caso 3: Correlación de Múltiples Jobs**
```bash
# Crear workflow con correlation_id
cargo run --bin hodei-jobs-cli -- job run \
  --name "Step 1" \
  --correlation-id "pipeline-123" \
  --command "sleep 2"

cargo run --bin hodei-jobs-cli -- job run \
  --name "Step 2" \
  --correlation-id "pipeline-123" \
  --command "sleep 2"

# Ver todo el workflow
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    aggregate_id,
    event_type,
    to_char(occurred_at, 'HH24:MI:SS') as time,
    payload->>'job_id' as job_id,
    actor
FROM domain_events
WHERE correlation_id = 'pipeline-123'
ORDER BY occurred_at;
"
```

### 🚨 **Alertas y Detección Automática de Problemas**

#### **Script de Verificación Automática**
```bash
#!/bin/bash
# Guardar como: scripts/check-job-health.sh

JOB_ID=$1
if [ -z "$JOB_ID" ]; then
    echo "❌ Uso: $0 <JOB_ID>"
    exit 1
fi

echo "🔍 Verificando salud del job: $JOB_ID"
echo "========================================"

# 1. Ver si el job existe
EXISTS=$(docker exec hodei-jobs-postgres psql -U postgres -t -c \
    "SELECT COUNT(*) FROM jobs WHERE id = '$JOB_ID';")

if [ "$EXISTS" -eq 0 ]; then
    echo "❌ Job no existe en la base de datos"
    exit 1
fi

echo "✅ Job existe"

# 2. Ver estado actual
STATE=$(docker exec hodei-jobs-postgres psql -U postgres -t -c \
    "SELECT state FROM jobs WHERE id = '$JOB_ID';")
echo "📊 Estado actual: $STATE"

# 3. Ver eventos
EVENT_COUNT=$(docker exec hodei-jobs-postgres psql -U postgres -t -c \
    "SELECT COUNT(*) FROM domain_events WHERE aggregate_id = '$JOB_ID' OR correlation_id = '$JOB_ID';")
echo "📡 Eventos registrados: $EVENT_COUNT"

# 4. Verificar secuencia
echo ""
echo "📋 Timeline de eventos:"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    to_char(occurred_at, 'HH24:MI:SS.MS') as time,
    event_type,
    COALESCE(actor, 'unknown') as actor
FROM domain_events
WHERE aggregate_id = '$JOB_ID' OR correlation_id = '$JOB_ID'
ORDER BY occurred_at;
"

# 5. Verificar coherencia
echo ""
echo "✅ Verificación completada"
```

### 📊 **Métricas de Consistencia**

```sql
-- Tasa de éxito de jobs por correlation_id
SELECT
    correlation_id,
    COUNT(DISTINCT aggregate_id) as total_jobs,
    SUM(CASE WHEN final_state = 'SUCCEEDED' THEN 1 ELSE 0 END) as succeeded,
    ROUND(
        100.0 * SUM(CASE WHEN final_state = 'SUCCEEDED' THEN 1 ELSE 0 END) / COUNT(DISTINCT aggregate_id),
        2
    ) as success_rate
FROM (
    SELECT
        correlation_id,
        aggregate_id,
        MAX(CASE WHEN event_type = 'JobStatusChanged' THEN
            CASE payload->>'new_state'
                WHEN 'SUCCEEDED' THEN 'SUCCEEDED'
                WHEN 'FAILED' THEN 'FAILED'
                WHEN 'CANCELLED' THEN 'CANCELLED'
                ELSE 'OTHER'
            END
        END) as final_state
    FROM domain_events
    WHERE correlation_id IS NOT NULL
      AND occurred_at > now() - interval '1 day'
    GROUP BY correlation_id, aggregate_id
) t
GROUP BY correlation_id
HAVING COUNT(DISTINCT aggregate_id) > 1
ORDER BY success_rate DESC;
```

### 🎯 **Mejores Prácticas**

1. **Siempre usar correlation_id** para jobs relacionados
2. **Verificar timeline** antes de investigar problemas
3. **Monitorear eventos por actor** para detectar patrones
4. **Validar coherencia secuencial** en transiciones de estado
5. **Usar logs estructurados** con correlation_id y actor

---

**💡 Tip Final:** El sistema de auditoría es tu mejor aliado para debug. Siempre comienza por `just debug-job-timeline <JOB_ID>` cuando algo falle. Te dará una visión completa del flujo y te permitirá identificar exactamente dónde se rompió la cadena de eventos.

