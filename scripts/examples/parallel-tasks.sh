#!/bin/bash
# =============================================================================
# Parallel Tasks Simulation - Concurrent job execution
# =============================================================================
# Simulates parallel task execution with progress tracking

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║          PARALLEL TASKS SIMULATION - Concurrency Test          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "📅 Started: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

WORK_DIR="/tmp/parallel_$$"
mkdir -p "$WORK_DIR"
trap "rm -rf $WORK_DIR" EXIT

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[1/4] Launching parallel workers..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Worker function
worker() {
    local id=$1
    local work_time=$((RANDOM % 5 + 3))
    echo "  [Worker-$id] Started (will run for ${work_time}s)"
    
    for i in $(seq 1 $work_time); do
        sleep 1
        echo "  [Worker-$id] Processing... step $i/$work_time"
    done
    
    echo "  [Worker-$id] ✓ Completed"
    echo "$id:completed:$work_time" > "$WORK_DIR/worker_$id.result"
}

# Launch workers in background
for w in 1 2 3 4; do
    worker $w &
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[2/4] Waiting for workers to complete..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

wait
echo ""
echo "  All workers finished!"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[3/4] Collecting results..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

total_time=0
for f in "$WORK_DIR"/*.result; do
    if [ -f "$f" ]; then
        result=$(cat "$f")
        worker_id=$(echo "$result" | cut -d: -f1)
        status=$(echo "$result" | cut -d: -f2)
        duration=$(echo "$result" | cut -d: -f3)
        echo "  Worker-$worker_id: $status (${duration}s)"
        total_time=$((total_time + duration))
    fi
done
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[4/4] Summary..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║              PARALLEL EXECUTION COMPLETED                      ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║  Workers:           4                                          ║"
echo "║  Total Work Time:   ${total_time}s (sequential would be)          ║"
echo "║  Actual Time:       ~8s (parallel execution)                   ║"
echo "║  Speedup:           ~$(echo "scale=1; $total_time / 8" | bc 2>/dev/null || echo "2")x                                        ║"
echo "║  Status:            ✅ SUCCESS                                 ║"
echo "╚════════════════════════════════════════════════════════════════╝"
