#!/bin/bash
# =============================================================================
# Rust Job Runner - Execute jobs with real-time log streaming
# =============================================================================

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║              RUST JOB RUNNER - TEST                            ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Test 1: Simple command
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 1: Simple Command"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Running: cargo run --bin stream-logs -- --name 'Test-1' --command 'echo Line 1; sleep 1; echo Line 2; sleep 1; echo Line 3'"
echo ""

cargo run --bin stream-logs -- \
    --name "Test-1" \
    --command "echo 'Line 1'; sleep 1; echo 'Line 2'; sleep 1; echo 'Line 3'"

echo ""
echo "✅ Test 1 complete!"
echo ""

# Test 2: With script file
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 2: Script File"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Create a test script
cat > /tmp/test-script.sh << 'EOF'
#!/bin/bash
echo "Starting test script..."
for i in {1..5}; do
    echo "[$i/5] Processing batch $i..."
    sleep 1
done
echo "Test script completed!"
EOF

echo "Running: cargo run --bin stream-logs -- --name 'Test-2' --script /tmp/test-script.sh"
echo ""

cargo run --bin stream-logs -- \
    --name "Test-2" \
    --script "/tmp/test-script.sh"

echo ""
echo "✅ Test 2 complete!"
echo ""

# Test 3: Longer job
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 3: Longer Job (10 seconds)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "Running: cargo run --bin stream-logs -- --name 'Test-3' --command 'for i in {1..10}; do echo Iteration \$i; sleep 1; done'"
echo ""

cargo run --bin stream-logs -- \
    --name "Test-3" \
    --command "for i in {1..10}; do echo 'Iteration $i'; sleep 1; done"

echo ""
echo "✅ Test 3 complete!"
echo ""

# Cleanup
rm -f /tmp/test-script.sh

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    ALL TESTS COMPLETE                          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
