#!/bin/bash
# =============================================================================
# Script de Diagnóstico del JobDispatcher
# =============================================================================
# Identifica por qué el JobDispatcher no está procesando la cola

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         DIAGNÓSTICO DEL JOB DISPATCHER                          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Función para imprimir con color
print_status() {
    local status=$1
    local message=$2
    if [ "$status" = "OK" ]; then
        echo -e "${GREEN}✅ $message${NC}"
    elif [ "$status" = "WARN" ]; then
        echo -e "${YELLOW}⚠️  $message${NC}"
    elif [ "$status" = "ERROR" ]; then
        echo -e "${RED}❌ $message${NC}"
    else
        echo "ℹ️  $message"
    fi
}

echo "1. Verificando Procesos del Sistema"
echo "=================================="

# Verificar si PostgreSQL está corriendo
if docker ps | grep -q hodei-jobs-postgres; then
    print_status "OK" "PostgreSQL container está corriendo"
else
    print_status "ERROR" "PostgreSQL container NO está corriendo"
    exit 1
fi

# Verificar servidor
if ps aux | grep -q "hodei-server-bin" | grep -v grep; then
    SERVER_PID=$(ps aux | grep "hodei-server-bin" | grep -v grep | awk '{print $2}' | head -1)
    print_status "OK" "Servidor está corriendo (PID: $SERVER_PID)"
else
    print_status "ERROR" "Servidor NO está corriendo"
    exit 1
fi

# Verificar worker
if ps aux | grep -q "hodei-worker-bin" | grep -v grep; then
    WORKER_PID=$(ps aux | grep "hodei-worker-bin" | grep -v grep | awk '{print $2}' | head -1)
    print_status "OK" "Worker está corriendo (PID: $WORKER_PID)"
else
    print_status "WARN" "Worker NO está corriendo"
fi

echo ""
echo "2. Verificando Base de Datos"
echo "============================"

# Verificar jobs en PENDING
PENDING_JOBS=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "SELECT COUNT(*) FROM jobs WHERE state = 'PENDING';" | xargs)
if [ "$PENDING_JOBS" -gt 0 ]; then
    print_status "WARN" "Jobs en PENDING: $PENDING_JOBS"
else
    print_status "OK" "No hay jobs en PENDING"
fi

# Verificar workers READY
READY_WORKERS=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "SELECT COUNT(*) FROM workers WHERE state = 'READY';" | xargs)
if [ "$READY_WORKERS" -gt 0 ]; then
    print_status "OK" "Workers READY: $READY_WORKERS"
else
    print_status "ERROR" "No hay workers READY"
fi

# Verificar cola
QUEUE_SIZE=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "SELECT COUNT(*) FROM job_queue;" | xargs)
if [ "$QUEUE_SIZE" -gt 0 ]; then
    print_status "WARN" "Jobs en cola: $QUEUE_SIZE"
else
    print_status "OK" "Cola vacía"
fi

echo ""
echo "3. Analizando Logs del Servidor"
echo "==============================="

echo "Buscando logs del JobController..."
if grep -q "JobController" /tmp/server.log; then
    print_status "OK" "JobController mencionado en logs"
    echo "Últimas 5 menciones:"
    grep -i "JobController" /tmp/server.log | tail -5 | sed 's/^/  /'
else
    print_status "WARN" "JobController NO encontrado en logs"
fi

echo ""
echo "Buscando logs del Dispatcher..."
if grep -qi "dispatcher\|dispatch" /tmp/server.log; then
    print_status "OK" "Dispatcher mencionado en logs"
else
    print_status "ERROR" "Dispatcher NO encontrado en logs"
fi

echo ""
echo "Buscando errores gRPC..."
ERROR_COUNT=$(grep -c "h2 protocol error" /tmp/server.log 2>/dev/null || echo "0")
if [ "$ERROR_COUNT" -gt 0 ]; then
    print_status "WARN" "Errores gRPC encontrados: $ERROR_COUNT"
    echo "Último error:"
    grep "h2 protocol error" /tmp/server.log | tail -1 | sed 's/^/  /'
else
    print_status "OK" "No hay errores gRPC"
fi

echo ""
echo "4. Verificando Configuración del Servidor"
echo "=========================================="

if [ -f .env ]; then
    echo "Variables de entorno relevantes:"
    grep -E "HODEI_JOB_CONTROLLER_ENABLED|HODEI_PROVISIONING_ENABLED|HODEI_DOCKER_ENABLED" .env | sed 's/^/  /'
else
    print_status "WARN" "Archivo .env no encontrado"
fi

echo ""
echo "5. Timeline de Eventos Recientes"
echo "================================="

echo "Últimos 5 eventos de dominio:"
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
"SELECT event_type, aggregate_id, actor, occurred_at FROM domain_events ORDER BY occurred_at DESC LIMIT 5;" \
| sed 's/^/  /'

echo ""
echo "Últimos 3 jobs:"
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
"SELECT id, state, attempts, created_at FROM jobs ORDER BY created_at DESC LIMIT 3;" \
| sed 's/^/  /'

echo ""
echo "6. Diagnóstico Final"
echo "===================="

if [ "$PENDING_JOBS" -gt 0 ] && [ "$READY_WORKERS" -gt 0 ] && [ "$QUEUE_SIZE" -gt 0 ]; then
    print_status "ERROR" "PROBLEMA DETECTADO:"
    echo "  - Hay $PENDING_JOBS jobs en PENDING"
    echo "  - Hay $READY_WORKERS workers disponibles"
    echo "  - Hay $QUEUE_SIZE jobs en la cola"
    echo "  - Pero NO se están procesando"
    echo ""
    echo "POSIBLES CAUSAS:"
    echo "  1. JobDispatcher loop no está ejecutándose"
    echo "  2. Error en la consulta de la cola"
    echo "  3. JobController.start() no fue llamado"
    echo ""
    echo "ACCIONES RECOMENDADAS:"
    echo "  1. Revisar logs en tiempo real: tail -f /tmp/server.log"
    echo "  2. Verificar que JobController.start() se ejecuta"
    echo "  3. Añadir logs de debug en dispatcher"
    echo "  4. Reiniciar el servidor"
    exit 1
else
    print_status "OK" "No se detectaron problemas obvios"
    exit 0
fi
