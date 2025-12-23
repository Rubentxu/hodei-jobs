#!/bin/bash
set -e

PORT=50051
MAX_WAIT=10

if [ -f ".env" ]; then
    echo "Loading environment from .env..."
    set -a
    source .env
    set +a
fi

echo "========================================="
echo "  Hodei Server Dev Startup"
echo "========================================="
echo ""

# Step 1: Kill any process using the port
echo "Step 1: Killing processes on port $PORT..."
PIDS=$(lsof -Pi :$PORT -sTCP:LISTEN -t 2>/dev/null || true)

if [ -n "$PIDS" ]; then
    echo "  Found processes: $PIDS"
    for PID in $PIDS; do
        echo "  Killing PID $PID..."
        kill -9 $PID 2>/dev/null || true
    done
else
    echo "  No processes found on port $PORT"
fi

# Step 2: Force kill with fuser
echo ""
echo "Step 2: Force killing with fuser..."
fuser -k ${PORT}/tcp 2>/dev/null || true

# Step 3: Wait and verify multiple times
echo ""
echo "Step 3: Waiting for port to be free..."
for i in $(seq 1 $MAX_WAIT); do
    if lsof -Pi :$PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
        echo "  Attempt $i/$MAX_WAIT: Port still in use, waiting..."
        sleep 1
    else
        echo "  ✅ Port $PORT is FREE!"
        break
    fi
done

# Final check
if lsof -Pi :$PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo ""
    echo "❌ ERROR: Port $PORT is still in use after $MAX_WAIT seconds"
    echo ""
    echo "Process info:"
    lsof -Pi :$PORT
    echo ""
    echo "Attempting final force kill..."
    lsof -Pi :$PORT -sTCP:LISTEN -t | xargs kill -9 2>/dev/null || true
    sleep 1

    if lsof -Pi :$PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
        echo "❌ FAILED to free port $PORT"
        exit 1
    fi
fi

echo ""
echo "Step 4: Compiling server..."
cargo build --package hodei-server-bin

if [ $? -ne 0 ]; then
    echo "❌ Compilation failed!"
    exit 1
fi

echo ""
echo "Step 5: Starting server on port $PORT..."
echo "========================================="
echo ""

# Start server (this will block)
cd crates/server/bin
exec cargo run --bin hodei-server-bin
