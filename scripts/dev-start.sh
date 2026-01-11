#!/bin/bash
# Robust development server startup script
# Ensures clean restart with proper port handling and container networking

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

# Get container network mode
get_network_mode() {
    local container_name=$1
    docker inspect "$container_name" --format '{{.HostConfig.NetworkMode}}' 2>/dev/null
}

# Get container IPs for proper networking (only for bridge mode)
get_container_ip() {
    local container_name=$1
    docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$container_name" 2>/dev/null
}

# Check if running inside Docker and get container IPs
POSTGRES_CONTAINER="hodei-jobs-postgres"
NATS_CONTAINER="hodei-jobs-nats"

# Detect if we need to use container IPs (localhost works for native, container IPs for bridged)
if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "$POSTGRES_CONTAINER"; then
    echo "🔗 Detected Docker containers, configuring container networking..."

    POSTGRES_NETWORK=$(get_network_mode "$POSTGRES_CONTAINER")
    NATS_NETWORK=$(get_network_mode "$NATS_CONTAINER")

    echo "   PostgreSQL network mode: $POSTGRES_NETWORK"
    echo "   NATS network mode: $NATS_NETWORK"

    # If containers use host network mode, use localhost
    if [ "$POSTGRES_NETWORK" = "host" ]; then
        echo "   PostgreSQL using host network, connecting to localhost"
        export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
    else
        # For bridge mode, use container IP
        POSTGRES_IP=$(get_container_ip "$POSTGRES_CONTAINER")
        if [ -n "$POSTGRES_IP" ] && [ "$POSTGRES_IP" != "invalid IP" ]; then
            echo "   PostgreSQL container IP: $POSTGRES_IP"
            export DATABASE_URL="postgres://postgres:postgres@${POSTGRES_IP}:5432/hodei_jobs"
        else
            echo "   Failed to get PostgreSQL IP, falling back to localhost"
            export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
        fi
    fi

    if [ "$NATS_NETWORK" = "host" ]; then
        echo "   NATS using host network, connecting to localhost"
        export HODEI_NATS_URLS="nats://localhost:4222"
    else
        # For bridge mode, use container IP
        NATS_IP=$(get_container_ip "$NATS_CONTAINER")
        if [ -n "$NATS_IP" ] && [ "$NATS_IP" != "invalid IP" ]; then
            echo "   NATS container IP: $NATS_IP"
            export HODEI_NATS_URLS="nats://${NATS_IP}:4222"
        else
            echo "   Failed to get NATS IP, falling back to localhost"
            export HODEI_NATS_URLS="nats://localhost:4222"
        fi
    fi
else
    echo "⚠️  Docker containers not detected, using localhost"
    export DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/hodei_jobs}"
    export HODEI_NATS_URLS="${HODEI_NATS_URLS:-nats://localhost:4222}"
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

# Start server with proper environment
echo "🚀 Starting server..."
echo "   DATABASE_URL: ${DATABASE_URL:-localhost}"
echo "   HODEI_NATS_URLS: ${HODEI_NATS_URLS:-nats://localhost:4222}"

cd crates/server/bin && cargo run --bin hodei-server-bin
