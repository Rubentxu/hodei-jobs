#!/bin/bash
# =============================================================================
# Simple test to verify Rust client can connect to the server
# =============================================================================

set -e

# Configuration
URL="localhost:50051"

echo "=== Testing Rust Log Client ==="
echo ""

# Check if Rust client exists
if [[ ! -x "/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs" ]]; then
    echo "❌ Rust client not found!"
    echo "Building it now..."
    cd /home/rubentxu/Proyectos/rust/package/hodei-job-platform
    cargo build --release --bin stream-logs 2>&1 | tail -3
    ln -sf /home/rubentxu/Proyectos/rust/package/hodei-job-platform/target/release/stream-logs /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs
    echo ""
fi

echo "✅ Rust client found at: /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs"
echo ""

# Check if API is running
echo "Checking if API is running..."
if ! grpcurl -plaintext "$URL" list &> /dev/null; then
    echo "❌ API is not running on $URL"
    echo "Please start the server first:"
    echo "  just dev-server"
    exit 1
fi

echo "✅ API is running on $URL"
echo ""

# Test with a fake job ID to see if client connects
echo "Testing client connection with fake job ID..."
echo "Command: /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs 00000000-0000-0000-0000-000000000000"
echo ""
echo "Expected: Client connects and waits (will timeout after ~10 seconds)"
echo "Press Ctrl+C after a few seconds if it hangs..."
echo ""

# Run with timeout to prevent hanging forever
timeout 10 stdbuf -oL -eL /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs 00000000-0000-0000-0000-000000000000 2>&1 || true

echo ""
echo "=== Test Complete ==="
echo ""
echo "If you saw the client try to connect (even with errors), the client works!"
echo "The issue is likely with log streaming or buffering."
