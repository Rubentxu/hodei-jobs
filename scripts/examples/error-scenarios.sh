#!/bin/bash
# =============================================================================
# Error Scenarios Test - Test error handling and stderr streaming
# =============================================================================
# Generates various error scenarios to test error handling

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║           ERROR SCENARIOS TEST - Resilience Check              ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "📅 Started: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[1/5] Testing stdout/stderr interleaving..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
for i in $(seq 1 5); do
    echo "  [stdout] Normal log message $i"
    echo "  [stderr] Warning: Non-critical issue $i" >&2
    sleep 0.3
done
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[2/5] Testing recoverable errors..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
for attempt in $(seq 1 3); do
    echo "  Attempt $attempt/3: Connecting to service..."
    sleep 0.5
    if [ $attempt -lt 3 ]; then
        echo "  ERROR: Connection timeout (retrying...)" >&2
    else
        echo "  ✓ Connected successfully on attempt $attempt"
    fi
done
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[3/5] Testing validation errors..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Validating input data..."
sleep 0.5
echo "  ERROR: Field 'email' is invalid: missing @ symbol" >&2
echo "  ERROR: Field 'age' is invalid: must be positive integer" >&2
echo "  ERROR: Field 'date' is invalid: format should be YYYY-MM-DD" >&2
echo "  ⚠️  3 validation errors found (continuing with valid records)"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[4/5] Testing warning accumulation..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
for i in $(seq 1 10); do
    echo "  Processing record $i..."
    if [ $((i % 3)) -eq 0 ]; then
        echo "  WARN: Record $i has deprecated format" >&2
    fi
    sleep 0.2
done
echo "  ✓ Processed 10 records (3 warnings)"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[5/5] Testing graceful degradation..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Checking primary service..."
sleep 0.5
echo "  ERROR: Primary service unavailable" >&2
echo "  Falling back to secondary service..."
sleep 0.5
echo "  ✓ Secondary service available"
echo "  Continuing with degraded functionality..."
sleep 0.5
echo ""

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║              ERROR SCENARIOS COMPLETED                         ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║  Scenarios Tested: 5                                           ║"
echo "║  Errors Generated: 8                                           ║"
echo "║  Warnings Generated: 6                                         ║"
echo "║  Status: ✅ ALL SCENARIOS HANDLED                              ║"
echo "╚════════════════════════════════════════════════════════════════╝"
