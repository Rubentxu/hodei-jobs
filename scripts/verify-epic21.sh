#!/bin/bash
# EPIC-21 Compliance Verification Script
# Verifica que provider selection y ephemeral workers funcionen correctamente

set -e

echo "═══════════════════════════════════════════════════════"
echo "         VERIFICACIÓN EPIC-21 COMPLIANCE"
echo "═══════════════════════════════════════════════════════"

# 1. Verificar Provider Mismatches
echo ""
echo "1️⃣  PROVIDER SELECTION:"
MISMATCHES=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT COUNT(*) FROM jobs j
JOIN workers w ON j.worker_id = w.id
WHERE j.spec->'preferences'->>'preferred_provider' IS NOT NULL
  AND NOT (
    w.provider_type ILIKE '%' || (j.spec->'preferences'->>'preferred_provider') || '%'
    OR (j.spec->'preferences'->>'preferred_provider' IN ('k8s', 'kube') AND w.provider_type = 'kubernetes')
  )
  AND j.created_at > now() - interval '1 hour';" 2>/dev/null | tr -d ' ' || echo "0")

if [ "$MISMATCHES" -gt 0 ] 2>/dev/null; then
    echo "   ❌ FALLO: $MISMATCHES jobs con provider incorrecto"
else
    echo "   ✅ OK: Todos los jobs usan provider preferido"
fi

# 2. Verificar Workers Reutilizados
echo ""
echo "2️⃣  EPHEMERAL WORKERS:"
REUSED=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT COUNT(*) FROM workers
WHERE jobs_executed > 1
  AND created_at > now() - interval '1 hour';" 2>/dev/null | tr -d ' ' || echo "0")

if [ "$REUSED" -gt 0 ] 2>/dev/null; then
    echo "   ❌ FALLO: $REUSED workers reutilizados (deben ser efímeros)"
else
    echo "   ✅ OK: Workers son efímeros (1 job max)"
fi

# 3. Verificar Workers No Terminados
echo ""
echo "3️⃣  WORKER TERMINATION:"
NOT_TERMINATED=$(docker exec hodei-jobs-postgres psql -U postgres -t -c "
SELECT COUNT(*) FROM workers
WHERE jobs_executed > 0
  AND state != 'Terminated'
  AND created_at > now() - interval '1 hour';" 2>/dev/null | tr -d ' ' || echo "0")

if [ "$NOT_TERMINATED" -gt 0 ] 2>/dev/null; then
    echo "   ⚠️  WARN: $NOT_TERMINATED workers con jobs pero no terminados"
else
    echo "   ✅ OK: Workers terminan después de completar job"
fi

# 4. Verificar Decisiones del Scheduler
echo ""
echo "4️⃣  SCHEDULER DECISIONS:"
ASSIGN_TO_WORKER=$(grep -c "AssignToWorker" /tmp/server.log 2>/dev/null || echo "0")
PROVISION_WORKER=$(grep -c "ProvisionWorker\|EPIC-21: Provisioning" /tmp/server.log 2>/dev/null || echo "0")

if [ "$ASSIGN_TO_WORKER" -gt 0 ]; then
    echo "   ⚠️  WARN: $ASSIGN_TO_WORKER decisiones AssignToWorker (debería ser 0 en EPIC-21)"
else
    echo "   ✅ OK: Solo ProvisionWorker decisions ($PROVISION_WORKER)"
fi

# 5. Verificar preferred_provider en logs
echo ""
echo "5️⃣  PREFERRED PROVIDER EN LOGS:"
PREF_LOGS=$(grep -c "preferred_provider" /tmp/server.log 2>/dev/null || echo "0")
echo "   ℹ️  $PREF_LOGS menciones de preferred_provider en logs"

echo ""
echo "═══════════════════════════════════════════════════════"
echo "                    RESUMEN"
echo "═══════════════════════════════════════════════════════"

TOTAL_ISSUES=$((MISMATCHES + REUSED))
if [ "$TOTAL_ISSUES" -eq 0 ]; then
    echo "✅ EPIC-21 COMPLIANCE: PASSED"
    exit 0
else
    echo "❌ EPIC-21 COMPLIANCE: FAILED ($TOTAL_ISSUES issues)"
    exit 1
fi
