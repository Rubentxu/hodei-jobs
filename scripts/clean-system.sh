#!/bin/bash
# Script para limpiar completamente el sistema Hodei
# Uso: ./scripts/clean-system.sh

echo "🧹 LIMPIANDO SISTEMA HODEI JOBS..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# 1. Matar todos los workers
echo "1. Matando procesos hodei-worker-bin..."
pkill -9 -f "hodei-worker-bin" 2>/dev/null || true
sleep 1

# 2. Matar servidor
echo "2. Matando proceso hodei-server-bin..."
pkill -9 -f "hodei-server-bin" 2>/dev/null || true
sleep 1

# 3. Limpiar contenedores Docker antiguos
echo "3. Limpiando contenedores Docker antiguos..."
WORKER_CONTAINERS=$(docker ps -aq --filter "name=hodei-worker" 2>/dev/null || true)
if [ ! -z "$WORKER_CONTAINERS" ]; then
    echo "   Eliminando $(echo "$WORKER_CONTAINERS" | wc -l) contenedores de workers..."
    docker rm -f $WORKER_CONTAINERS 2>/dev/null || true
fi

# 4. Limpiar base de datos (solo si el contenedor está corriendo)
echo "4. Limpiando base de datos..."
if docker ps | grep -q hodei-jobs-postgres; then
    docker exec hodei-jobs-postgres psql -U postgres -c "
    DROP TABLE IF EXISTS job_queue CASCADE;
    DROP TABLE IF EXISTS jobs CASCADE;
    DROP TABLE IF EXISTS workers CASCADE;
    DROP TABLE IF EXISTS worker_bootstrap_tokens CASCADE;
    DROP TABLE IF EXISTS provider_configs CASCADE;
    " 2>/dev/null || echo "   ⚠️  Error limpiando DB (¿contenedor PostgreSQL corriendo?)"
else
    echo "   ⚠️  PostgreSQL no está corriendo, saltando limpieza de DB"
fi

# 5. Limpiar logs temporales
echo "5. Limpiando logs temporales..."
rm -f /tmp/server*.log 2>/dev/null || true
rm -f /tmp/worker*.log 2>/dev/null || true

echo ""
echo "✅ SISTEMA LIMPIADO COMPLETAMENTE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "💡 Siguiente paso: ./scripts/restart-system.sh"
echo ""
