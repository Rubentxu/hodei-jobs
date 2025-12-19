#!/bin/bash
# =============================================================================
# High-Volume Log Generator - Stress test for log streaming
# =============================================================================
# Generates many log lines to test log streaming performance

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         HIGH-VOLUME LOG GENERATOR - Streaming Test             ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "📅 Started: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

TOTAL_LINES=500
BATCH_SIZE=50

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Configuration:"
echo "  Total lines: $TOTAL_LINES"
echo "  Batch size: $BATCH_SIZE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

LEVELS=("DEBUG" "INFO" "INFO" "INFO" "WARN" "ERROR")
SERVICES=("api-gateway" "user-service" "order-service" "payment-service" "auth-service")
ACTIONS=("request_received" "processing" "db_query" "cache_hit" "cache_miss" "response_sent" "validation" "transform")

for batch in $(seq 1 $((TOTAL_LINES / BATCH_SIZE))); do
    echo "━━━ Batch $batch/$((TOTAL_LINES / BATCH_SIZE)) ━━━"
    
    for i in $(seq 1 $BATCH_SIZE); do
        line_num=$(( (batch - 1) * BATCH_SIZE + i ))
        level=${LEVELS[$((RANDOM % ${#LEVELS[@]}))]}
        service=${SERVICES[$((RANDOM % ${#SERVICES[@]}))]}
        action=${ACTIONS[$((RANDOM % ${#ACTIONS[@]}))]}
        request_id=$(printf "%08x" $((RANDOM * RANDOM)))
        duration=$((RANDOM % 1000))
        
        timestamp=$(date '+%Y-%m-%d %H:%M:%S.%3N')
        
        case $level in
            "DEBUG")
                echo "[$timestamp] $level  [$service] req=$request_id action=$action details=\"Processing step $i\""
                ;;
            "INFO")
                echo "[$timestamp] $level   [$service] req=$request_id action=$action duration=${duration}ms status=ok"
                ;;
            "WARN")
                echo "[$timestamp] $level   [$service] req=$request_id action=$action message=\"Slow response detected\" duration=${duration}ms" >&2
                ;;
            "ERROR")
                echo "[$timestamp] $level  [$service] req=$request_id action=$action error=\"Connection timeout\" retry=true" >&2
                ;;
        esac
    done
    
    sleep 0.1
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Summary:"
echo "  Lines generated: $TOTAL_LINES"
echo "  Batches: $((TOTAL_LINES / BATCH_SIZE))"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  ✅ LOG GENERATION COMPLETED                                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
