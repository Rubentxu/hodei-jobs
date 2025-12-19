#!/bin/bash
# =============================================================================
# Battery of Test Jobs - Comprehensive Testing Suite
# =============================================================================

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║            BATTERY OF TEST JOBS - COMPREHENSIVE SUITE          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Test counter
TEST_NUM=1

# Function to run a test
run_test() {
    local name="$1"
    local description="$2"
    local command="$3"

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "TEST $TEST_NUM: $name"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Description: $description"
    echo "Command: $command"
    echo ""

    # Run the test
    cargo run --bin stream-logs -- $command

    echo ""
    echo "✅ Test $TEST_NUM complete!"
    echo ""

    ((TEST_NUM++))
}

# Test 1: Simple Command
run_test "Simple Command" \
    "Basic echo command with sleep" \
    '--name "Test-Simple" --command "echo '\''Hello World'\''; sleep 1; echo '\''Done'\''"'

# Test 2: Script File
echo "Creating test script..."
cat > /tmp/test-script.sh << 'EOF'
#!/bin/bash
echo "Test script starting..."
for i in {1..5}; do
    echo "[$i/5] Processing item $i..."
    sleep 0.5
done
echo "Test script completed!"
EOF

run_test "Script File Execution" \
    "Execute a bash script file" \
    '--name "Test-Script" --script /tmp/test-script.sh'

# Test 3: ETL Pipeline Simulation
run_test "ETL Pipeline" \
    "Simulates an ETL process with three stages" \
    '--name "Test-ETL" --command "echo '\''=== ETL PIPELINE ==='\''; echo '\''[1/3] Extracting data from source...'\''; sleep 1; echo '\''Extracted 10,000 records'\''; echo ''; echo '\''[2/3] Transforming data...'\''; sleep 1; echo '\''Data transformed successfully'\''; echo ''; echo '\''[3/3] Loading to destination...'\''; sleep 1; echo '\''All data loaded!'\''; echo ''; echo '\''ETL Pipeline Completed!'\''"'

# Test 4: CI/CD Build Process
run_test "CI/CD Build" \
    "Simulates a CI/CD build pipeline" \
    '--name "Test-CICD" --command "echo '\''=== CI/CD BUILD PIPELINE ==='\''; echo '\''Stage 1: Checkout code'\''; sleep 1; echo '\''✓ Code checked out'\''; echo ''; echo '\''Stage 2: Install dependencies'\''; sleep 1; echo '\''✓ Dependencies installed'\''; echo ''; echo '\''Stage 3: Run tests'\''; sleep 2; echo '\''✓ All tests passed'\''; echo ''; echo '\''Stage 4: Build application'\''; sleep 1; echo '\''✓ Build successful'\''; echo ''; echo '\''Stage 5: Deploy'\''; sleep 1; echo '\''✓ Deployed successfully'\''; echo ''; echo '\''=== PIPELINE COMPLETE ==='\''"'

# Test 5: Data Processing
run_test "Data Processing" \
    "Simulates processing 50 data records" \
    '--name "Test-Data-Processing" --command "echo '\''Starting data processing...'\''; for i in {1..50}; do if [ \$((i % 10)) -eq 0 ]; then echo \"Processed \$i/50 records (10% complete)\"; fi; sleep 0.2; done; echo ''; echo '\''All 50 records processed successfully!'\''; echo '\''Data processing complete!'\''"'

# Test 6: Long Running Job
run_test "Long Running Job" \
    "A job that runs for 15 seconds" \
    '--name "Test-Long-Running" --command "for i in {1..15}; do echo \"[\$i/15] Elapsed time: \${i}s\"; sleep 1; done; echo ''; echo '\''Job completed after 15 seconds!'\''"'

# Test 7: Error Handling
run_test "Error Handling" \
    "Demonstrates error handling and recovery" \
    '--name "Test-Error-Handling" --command "echo '\''Starting job...'\''; echo '\''Step 1: Initialize'\''; sleep 1; echo ''; echo '\''Step 2: Process data'\''; sleep 1; echo ''; echo '\''Step 3: Try invalid operation'\''; ls /nonexistent-directory 2>&1 || echo '\''Error caught, continuing...'\''; echo ''; echo '\''Step 4: Cleanup'\''; sleep 1; echo ''; echo '\''Job completed with error handling!'\''"'

# Test 8: Multi-Stage Pipeline
run_test "Multi-Stage Pipeline" \
    "A complex multi-stage data pipeline" \
    '--name "Test-Multi-Stage" --command "echo '\''=== MULTI-STAGE PIPELINE ==='\''; echo ''; echo '\''[Stage 1/5] Data Extraction'\''; for i in {1..5}; do echo \"  Extracting batch \$i/5...\"; sleep 0.5; done; echo ''; echo '\''[Stage 2/5] Validation'\''; for i in {1..3}; do echo \"  Validating batch \$i/3...\"; sleep 0.5; done; echo ''; echo '\''[Stage 3/5] Transformation'\''; for i in {1..4}; do echo \"  Transforming batch \$i/4...\"; sleep 0.5; done; echo ''; echo '\''[Stage 4/5] Quality Checks'\''; for i in {1..3}; do echo \"  Running QC \$i/3...\"; sleep 0.5; done; echo ''; echo '\''[Stage 5/5] Loading'\''; for i in {1..5}; do echo \"  Loading batch \$i/5...\"; sleep 0.5; done; echo ''; echo '\''=== PIPELINE COMPLETE ==='\''; echo '\''All stages executed successfully!'\''"'

# Test 9: Memory/Resource Intensive
run_test "Resource Usage" \
    "Simulates a job with specific resource requirements" \
    '--name "Test-Resources" --command "echo '\''Starting resource-intensive job...'\''; echo ''; echo '\''Allocating 2GB memory...'\''; echo '\''Using 4 CPU cores...'\''; for i in {1..10}; do echo \"[\$i/10] Processing with allocated resources...\"; sleep 0.5; done; echo ''; echo '\''Job completed within resource limits!'\''; echo '\''Resource usage: CPU 80%, Memory 1.5GB'\''" \
    --cpu 4.0 \
    --memory 2147483648

# Test 10: Custom Timeout
run_test "Custom Timeout" \
    "Job with custom timeout setting" \
    '--name "Test-Timeout" --command "echo '\''Starting job with custom timeout...'\''; for i in {1..5}; do echo \"[\$i/5] Working...\"; sleep 1; done; echo ''; echo '\''Job completed before timeout!'\''" \
    --timeout 300s

# Cleanup
rm -f /tmp/test-script.sh

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║            BATTERY TESTS COMPLETED SUCCESSFULLY                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "📊 Summary:"
echo "   Total Tests: $((TEST_NUM - 1))"
echo "   All tests executed with real-time log streaming"
echo ""
echo "✅ All tests passed!"
