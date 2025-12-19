#!/bin/bash
# Robust development server startup script
# Ensures clean restart with proper port handling

set -e

echo "🔍 Checking for existing server on port 50051..."

# Function to check if port is in use
check_port() {
    lsof -Pi :50051 -sTCP:LISTEN -t >/dev/null 2>&1
}

# Function to wait for port to be free
wait_for_port() {
    local max_attempts=10
    local attempt=1

    while check_port; do
        if [ $attempt -eq 1 ]; then
            echo "⚠️  Port 50051 is in use, stopping existing server..."
        fi

        # Try graceful kill first
        pkill -f "hodei-server" 2>/dev/null || true
        sleep 1

        # If still in use, force kill
        if check_port; then
            echo "   Forcing process termination..."
            fuser -k 50051/tcp 2>/dev/null || true
            sleep 1
        fi

        if ! check_port; then
            echo "✅ Port 50051 is now free"
            return 0
        fi

        if [ $attempt -eq $max_attempts ]; then
            echo "❌ Failed to free port 50051 after $max_attempts attempts"
            exit 1
        fi

        echo "   Waiting... (attempt $attempt/$max_attempts)"
        attempt=$((attempt + 1))
    done

    echo "✅ Port 50051 is free"
}

# Check if server is already running
if check_port; then
    wait_for_port
else
    echo "✅ Port 50051 is free"
fi

# Compile server
echo "🔨 Recompiling server..."
cargo build --package hodei-server-bin

if [ $? -ne 0 ]; then
    echo "❌ Compilation failed"
    exit 1
fi

# Wait one more time to be absolutely sure
sleep 1

# Start server
echo "🚀 Starting server..."
cd crates/server/bin && cargo run --bin hodei-server-bin
