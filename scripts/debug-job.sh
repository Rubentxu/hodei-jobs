#!/bin/bash
# Script de Diagnóstico Completo de Job
# Uso: ./scripts/debug-job.sh <JOB_ID>

if [ -z "$1" ]; then
    echo "❌ Uso: $0 <JOB_ID>"
    echo "   Ejemplo: $0 584a465b-d208-4a05-beef-8671b9bc2805"
    exit 1
fi

JOB_ID=$1

echo "╔════════════════════════════════════════════════════════════╗"
echo "║         DIAGNÓSTICO COMPLETO DE JOB: $JOB_ID"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

echo "1️⃣  ESTADO DEL JOB:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    'ID: ' || id || ' | Estado: ' || state || ' | Intentos: ' || attempts || ' | Creado: ' || created_at as info,
    CASE
        WHEN state = 'PENDING' THEN '✅ Estado correcto para cola'
        WHEN state = 'SCHEDULED' THEN '⚠️  Estado inesperado (debería ser PENDING hasta confirmación)'
        WHEN state = 'RUNNING' THEN '▶️  Ejecutándose'
        WHEN state = 'SUCCEEDED' THEN '✅ Completado exitosamente'
        WHEN state = 'FAILED' THEN '❌ Falló'
        ELSE '❓ Estado desconocido'
    END as status_check
FROM jobs
WHERE id = '$JOB_ID';
" 2>/dev/null

echo ""
echo "2️⃣  EN COLA:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    CASE
        WHEN jq.job_id IS NOT NULL THEN '✅ SÍ - Está en job_queue (enqueued: ' || jq.enqueued_at || ')'
        ELSE '❌ NO - No está en job_queue (problema: dispatcher no lo encoló o ya fue dequeueado)'
    END as queue_status,
    j.state as job_state
FROM jobs j
LEFT JOIN job_queue jq ON j.id = jq.job_id
WHERE j.id = '$JOB_ID';
" 2>/dev/null

echo ""
echo "3️⃣  WORKERS DISPONIBLES:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    COUNT(*) as workers_ready,
    CASE
        WHEN COUNT(*) = 0 THEN '❌ CRÍTICO: No hay workers disponibles'
        WHEN COUNT(*) < 2 THEN '⚠️  ADVERTENCIA: Pocos workers disponibles'
        ELSE '✅ OK: Suficientes workers disponibles'
    END as availability_check
FROM workers
WHERE state = 'READY' AND last_heartbeat > now() - interval '30 seconds';
" 2>/dev/null

echo ""
echo "4️⃣  ESPECIFICACIÓN DEL JOB:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT jsonb_pretty(spec) as job_spec
FROM jobs
WHERE id = '$JOB_ID';
" 2>/dev/null

echo ""
echo "5️⃣  WORKERS RECIENTES (últimos 2 min):"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    id,
    state,
    last_heartbeat,
    EXTRACT(EPOCH FROM (now() - last_heartbeat)) as seconds_ago,
    current_job_id,
    CASE
        WHEN last_heartbeat > now() - interval '30 seconds' THEN '✅ Heartbeat OK'
        WHEN last_heartbeat > now() - interval '2 minutes' THEN '⚠️  Heartbeat lento'
        ELSE '❌ Worker desconectado'
    END as heartbeat_status
FROM workers
WHERE last_heartbeat > now() - interval '2 minutes'
ORDER BY last_heartbeat DESC
LIMIT 5;
" 2>/dev/null

echo ""
echo "6️⃣  PROVEEDORES CONFIGURADOS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    name,
    provider_type,
    status,
    CASE
        WHEN status = 'ACTIVE' THEN '✅ OK'
        ELSE '❌ Provider inactivo'
    END as provider_check
FROM provider_configs;
" 2>/dev/null

echo ""
echo "7️⃣  TOKENS BOOTSTRAP DISPONIBLES:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    COUNT(*) as total_tokens,
    COUNT(CASE WHEN consumed_at IS NULL AND expires_at > now() THEN 1 END) as tokens_disponibles,
    COUNT(CASE WHEN consumed_at IS NOT NULL THEN 1 END) as tokens_consumidos,
    COUNT(CASE WHEN expires_at <= now() THEN 1 END) as tokens_expirados
FROM worker_bootstrap_tokens;
" 2>/dev/null

echo ""
echo "8️⃣  RESUMEN DEL DIAGNÓSTICO:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Obtener estado actual del job
JOB_STATE=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "SELECT state FROM jobs WHERE id = '$JOB_ID';" 2>/dev/null | xargs)
IN_QUEUE=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "SELECT COUNT(*) FROM job_queue WHERE job_id = '$JOB_ID';" 2>/dev/null | xargs)
WORKERS_READY=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "SELECT COUNT(*) FROM workers WHERE state = 'READY' AND last_heartbeat > now() - interval '30 seconds';" 2>/dev/null | xargs)

echo "   Estado actual: $JOB_STATE"
echo "   En cola: $IN_QUEUE"
echo "   Workers listos: $WORKERS_READY"
echo ""

if [ "$JOB_STATE" = "PENDING" ] && [ "$IN_QUEUE" = "0" ]; then
    echo "   🔴 PROBLEMA DETECTADO: Job PENDING pero no en cola"
    echo "   📋 Acción: Verificar JobDispatcher está corriendo"
    echo "   🔍 Comando: tail -f /tmp/server.log | grep 'JobDispatcher'"
elif [ "$JOB_STATE" = "PENDING" ] && [ "$IN_QUEUE" = "1" ] && [ "$WORKERS_READY" = "0" ]; then
    echo "   🟡 PROBLEMA DETECTADO: Job en cola pero no hay workers"
    echo "   📋 Acción: Verificar registro de workers"
    echo "   🔍 Comando: HODEI_OTP_TOKEN=<token> cargo run --bin hodei-worker-bin"
elif [ "$WORKERS_READY" = "0" ]; then
    echo "   🔴 CRÍTICO: No hay workers disponibles"
    echo "   📋 Acción: Iniciar al menos un worker con token válido"
elif [ "$JOB_STATE" = "RUNNING" ]; then
    echo "   🟢 JOB EJECUTÁNDOSE CORRECTAMENTE"
    echo "   📋 Esperar logs del worker..."
elif [ "$JOB_STATE" = "SUCCEEDED" ]; then
    echo "   ✅ JOB COMPLETADO EXITOSAMENTE"
elif [ "$JOB_STATE" = "FAILED" ]; then
    echo "   ❌ JOB FALLÓ"
    echo "   📋 Verificar logs del worker para detalles"
else
    echo "   ❓ Estado: $JOB_STATE - Revisar logs del servidor"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "   💡 Tip: Usa 'just logs-server' y 'just logs-worker' para ver logs en tiempo real"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
