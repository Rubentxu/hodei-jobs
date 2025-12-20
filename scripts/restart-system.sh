#!/bin/bash
# Script para reiniciar completamente el sistema Hodei
# Uso: ./scripts/restart-system.sh

echo "🚀 REINICIANDO SISTEMA HODEI JOBS..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# 1. Limpiar sistema anterior
echo "1. Limpiando sistema anterior..."
./scripts/clean-system.sh
sleep 2

echo ""
echo "2. Levantando PostgreSQL..."

# Verificar si PostgreSQL ya está corriendo
if docker ps | grep -q hodei-jobs-postgres; then
    echo "   ✅ PostgreSQL ya está corriendo"
else
    echo "   📦 Iniciando contenedor PostgreSQL..."
    docker run -d --name hodei-jobs-postgres \
        -e POSTGRES_PASSWORD=postgres \
        -e POSTGRES_USER=postgres \
        -e POSTGRES_DB=postgres \
        -p 5432:5432 \
        postgres:16-alpine

    echo "   ⏳ Esperando a que PostgreSQL esté listo..."
    sleep 5

    # Verificar que esté listo
    RETRY=0
    until docker exec hodei-jobs-postgres pg_isready -U postgres > /dev/null 2>&1; do
        RETRY=$((RETRY + 1))
        if [ $RETRY -gt 10 ]; then
            echo "   ❌ ERROR: PostgreSQL no responde después de 10 intentos"
            exit 1
        fi
        echo "   ⏳ Esperando... (intento $RETRY/10)"
        sleep 1
    done
    echo "   ✅ PostgreSQL listo"
fi

echo ""
echo "3. Configurando provider Docker..."

docker exec hodei-jobs-postgres psql -U postgres << 'EOF' 2>/dev/null || echo "   ⚠️  Error configurando provider (¿tabla existe?)"
INSERT INTO provider_configs (id, name, provider_type, config, status, priority, max_workers, tags, metadata, created_at, updated_at)
VALUES (
  'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
  'Docker',
  'Docker',
  '{"socket_path": "/var/run/docker.sock"}',
  'ACTIVE',
  0,
  10,
  '[]'::jsonb,
  '{}'::jsonb,
  now(),
  now()
) ON CONFLICT (id) DO UPDATE SET
  name = EXCLUDED.name,
  config = EXCLUDED.config,
  status = EXCLUDED.status,
  updated_at = now();
EOF

echo ""
echo "4. Creando bootstrap token para worker..."

# Crear token y capturar output
TOKEN_OUTPUT=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "
INSERT INTO worker_bootstrap_tokens (token, worker_id, expires_at)
VALUES (gen_random_uuid(), '00000000-0000-0000-0000-000000000000', now() + interval '1 hour')
RETURNING token;
" 2>/dev/null)

if [ ! -z "$TOKEN_OUTPUT" ]; then
    TOKEN=$(echo "$TOKEN_OUTPUT" | xargs)
    echo "   ✅ Token creado: ${TOKEN:0:8}..."
else
    echo "   ❌ ERROR: No se pudo crear el token"
    exit 1
fi

echo ""
echo "5. Levantando servidor..."

# Configurar variable de entorno
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/postgres"
export RUST_LOG=hodei_server_application=DEBUG

# Iniciar servidor
cargo run --bin hodei-server-bin > /tmp/server.log 2>&1 &
SERVER_PID=$!

echo "   📦 Servidor iniciado (PID: $SERVER_PID)"
echo "   ⏳ Esperando a que el servidor esté listo..."

# Esperar a que el servidor responda
RETRY=0
until curl -s http://localhost:50051 > /dev/null 2>&1 || [ $RETRY -gt 30 ]; do
    RETRY=$((RETRY + 1))
    if [ $RETRY -eq 1 ]; then
        echo "   ⏳ Esperando servidor..."
    fi
    sleep 1
done

if [ $RETRY -gt 30 ]; then
    echo "   ⚠️  Advertencia: Servidor no responde después de 30s"
else
    echo "   ✅ Servidor listo"
fi

echo ""
echo "6. Levantando worker..."

# Configurar variables para worker
export HODEI_OTP_TOKEN=$TOKEN
export HODEI_SERVER_ADDRESS=localhost:50051
export RUST_LOG=hodei_worker_application=DEBUG

# Iniciar worker
cargo run --bin hodei-worker-bin > /tmp/worker.log 2>&1 &
WORKER_PID=$!

echo "   📦 Worker iniciado (PID: $WORKER_PID)"
echo "   ⏳ Esperando registro del worker..."

# Esperar a que el worker se registre
sleep 5

WORKER_REGISTERED=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT COUNT(*) FROM workers WHERE state IN ('READY', 'BUSY');
" 2>/dev/null | xargs)

if [ "$WORKER_REGISTERED" != "0" ]; then
    echo "   ✅ Worker registrado correctamente"
else
    echo "   ⚠️  Advertencia: Worker no se ha registrado aún"
    echo "   🔍 Revisar: tail -f /tmp/worker.log"
fi

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           SISTEMA REINICIADO EXITOSAMENTE                  ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "📋 INFORMACIÓN DEL SISTEMA:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "   🔑 Token Bootstrap: $TOKEN"
echo "   🖥️  Servidor PID:    $SERVER_PID"
echo "   👷 Worker PID:       $WORKER_PID"
echo "   📊 PostgreSQL:       Contenedor Docker"
echo "   📁 Logs Server:      /tmp/server.log"
echo "   📁 Logs Worker:      /tmp/worker.log"
echo ""
echo "🚀 PRÓXIMOS PASOS:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "   1. Verificar estado:    just debug-system"
echo "   2. Ver workers:         just debug-workers"
echo "   3. Probar job:          just job-data-processing"
echo "   4. Ver logs servidor:   just logs-server"
echo "   5. Ver logs worker:     just logs-worker"
echo ""
echo "💡 Para debuggear un job específico:"
echo "   just job-data-processing"
echo "   # Copiar el Job ID de la salida"
echo "   just debug-job <JOB_ID>"
echo ""
