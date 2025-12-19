#!/bin/bash
set -euo pipefail

echo "⚠️  Error Handling Demo"
echo "======================="
echo ""

echo "📋 Demo Scenarios:"
echo "   - Successful operations"
echo "   - Recoverable errors"
echo "   - Non-recoverable errors"
echo "   - Cleanup operations"
echo ""

echo "📊 Phase 1: Successful Operations"
echo "   → Task 1: Reading configuration..."
sleep 1
echo "   ✓ Configuration loaded"

echo "   → Task 2: Connecting to database..."
sleep 1
echo "   ✓ Database connected"

echo "   → Task 3: Processing data..."
sleep 1
echo "   ✓ Data processed successfully"

echo ""
echo "📊 Phase 2: Recoverable Error"
echo "   → Attempting operation with transient error..."
sleep 2
echo "   ⚠️  Transient error detected: Connection timeout"
echo "   → Retrying operation..."
sleep 1
echo "   ✓ Retry successful"

echo ""
echo "📊 Phase 3: Non-Recoverable Error"
echo "   → Attempting critical operation..."
sleep 1
echo "   → Validating input parameters..."
echo "   ✗ ERROR: Invalid parameter 'timeout=-1'"
echo "   → Attempting to recover..."
sleep 1
echo "   ✗ Recovery failed: Invalid parameter cannot be fixed"
echo "   → Cleaning up resources..."
sleep 1
echo "   ✓ Resources cleaned up"
echo "   → Exiting with error code 42"
exit 42
