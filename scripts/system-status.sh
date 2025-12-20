#!/bin/bash
# Dashboard de Estado del Sistema Hodei Jobs
# Uso: ./scripts/system-status.sh

clear
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           ESTADO DEL SISTEMA HODEI JOBS                    ║"
echo "║                   $(date '+%Y-%m-%d %H:%M:%S')                    ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

echo "📊 JOBS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
  state,
  COUNT(*) as count,
  ROUND(COUNT(*) * 100.0 / SUM(COUNT(*)) OVER (), 2) as percentage
FROM jobs
GROUP BY state
ORDER BY
  CASE state
    WHEN 'PENDING' THEN 1
    WHEN 'RUNNING' THEN 2
    WHEN 'SUCCEEDED' THEN 3
    WHEN 'FAILED' THEN 4
    WHEN 'SCHEDULED' THEN 5
    WHEN 'CANCELLED' THEN 6
    ELSE 7
  END;
" 2>/dev/null || echo "❌ Error conectando a la base de datos"

echo ""
echo "👥 WORKERS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
  state,
  COUNT(*) as count,
  CASE
    WHEN state = 'READY' THEN '✅ Listo para ejecutar jobs'
    WHEN state = 'BUSY' THEN '🔄 Ejecutando job'
    WHEN state = 'OFFLINE' THEN '❌ Desconectado'
    ELSE '❓ Estado desconocido'
  END as description
FROM workers
GROUP BY state
ORDER BY state;
" 2>/dev/null || echo "❌ Error conectando a la base de datos"

echo ""
echo "📋 COLA DE JOBS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
QUEUE_COUNT=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "SELECT COUNT(*) FROM job_queue;" 2>/dev/null | xargs)
echo "   Total de jobs en cola: $QUEUE_COUNT"

if [ ! -z "$QUEUE_COUNT" ] && [ "$QUEUE_COUNT" -gt 0 ]; then
    echo ""
    echo "   Jobs en cola (más recientes):"
    docker exec hodei-jobs-postgres psql -U postgres -c "
    SELECT
      jq.job_id,
      jq.enqueued_at,
      EXTRACT(EPOCH FROM (now() - jq.enqueued_at)) as seconds_in_queue,
      j.state
    FROM job_queue jq
    JOIN jobs j ON jq.job_id = j.id
    ORDER BY jq.enqueued_at DESC
    LIMIT 5;
    " 2>/dev/null || true
fi

echo ""
echo "⚙️  PROVIDERS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
  name,
  provider_type,
  status,
  max_workers,
  CASE
    WHEN status = 'ACTIVE' THEN '✅ Activo'
    ELSE '❌ Inactivo'
  END as status_check
FROM provider_configs
ORDER BY name;
" 2>/dev/null || echo "❌ Error conectando a la base de datos"

echo ""
echo "🔑 TOKENS BOOTSTRAP:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
  COUNT(*) as total,
  COUNT(CASE WHEN consumed_at IS NULL AND expires_at > now() THEN 1 END) as disponibles,
  COUNT(CASE WHEN consumed_at IS NOT NULL THEN 1 END) as consumidos,
  COUNT(CASE WHEN expires_at <= now() THEN 1 END) as expirados
FROM worker_bootstrap_tokens;
" 2>/dev/null || echo "❌ Error conectando a la base de datos"

echo ""
echo "🔄 PROCESOS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
SERVER_PID=$(pgrep -f "hodei-server-bin" | head -1)
WORKER_PID=$(pgrep -f "hodei-worker-bin" | head -1)

if [ ! -z "$SERVER_PID" ]; then
    echo "   ✅ Servidor corriendo (PID: $SERVER_PID)"
else
    echo "   ❌ Servidor NO está corriendo"
fi

WORKER_COUNT=$(pgrep -c -f "hodei-worker-bin")
if [ ! -z "$WORKER_COUNT" ] && [ "$WORKER_COUNT" -gt 0 ]; then
    echo "   ✅ Workers corriendo ($WORKER_COUNT procesos)"
else
    echo "   ❌ Workers NO están corriendo"
fi

DOCKER_POSTGRES=$(docker ps -q --filter "name=hodei-jobs-postgres" 2>/dev/null)
if [ ! -z "$DOCKER_POSTGRES" ]; then
    echo "   ✅ PostgreSQL corriendo"
else
    echo "   ❌ PostgreSQL NO está corriendo"
fi

DOCKER_WORKERS=$(docker ps -q --filter "name=hodei-worker" 2>/dev/null | wc -l)
if [ ! -z "$DOCKER_WORKERS" ] && [ "$DOCKER_WORKERS" -gt 0 ]; then
    echo "   📦 Workers Docker: $DOCKER_WORKERS contenedores"
fi

echo ""
echo "📈 MÉTRICAS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
  (SELECT COUNT(*) FROM jobs WHERE created_at > now() - interval '1 hour') as jobs_ultima_hora,
  (SELECT COUNT(*) FROM jobs WHERE state = 'RUNNING') as jobs_ejecutandose,
  (SELECT COUNT(*) FROM jobs WHERE state = 'FAILED' AND created_at > now() - interval '1 hour') as jobs_fallidos_ultima_hora,
  ROUND(
    CASE
      WHEN (SELECT COUNT(*) FROM jobs WHERE created_at > now() - interval '1 hour') > 0
      THEN (SELECT COUNT(*) * 100.0 FROM jobs WHERE state = 'SUCCEEDED' AND created_at > now() - interval '1 hour') /
           (SELECT COUNT(*) FROM jobs WHERE created_at > now() - interval '1 hour')
      ELSE 0
    END, 2
  ) as tasa_exito_ultima_hora_pct;
" 2>/dev/null || echo "❌ Error conectando a la base de datos"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "   💡 Comandos útiles:"
echo "      just debug-system        - Mostrar este dashboard"
echo "      just debug-jobs          - Ver jobs PENDING"
echo "      just debug-workers       - Ver workers"
echo "      just logs-server         - Ver logs del servidor"
echo "      just logs-worker         - Ver logs del worker"
echo "      just restart-all         - Reiniciar todo el sistema"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
