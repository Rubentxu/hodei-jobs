#!/bin/bash
set -e

SERVER_BIN="/usr/local/bin/hodei-jobs-server"
SYNC_DIR="/tmp/server-bin"
PID_FILE="/tmp/server.pid"

echo "🚀 Hodei Jobs Server - DevSpace Mode"
echo "===================================="

# Wait for DevSpace to sync the binary
echo "⏳ Waiting for binary sync..."
while [ ! -f "$SYNC_DIR" ]; do
    sleep 1
done

echo "✅ Binary synced!"

# Copy and make executable
cp "$SYNC_DIR" "$SERVER_BIN"
chmod +x "$SERVER_BIN"

# Write PID for reload support
echo $$ > "$PID_FILE"

echo "📦 Binary info:"
ls -la "$SERVER_BIN"
file "$SERVER_BIN"

echo ""
echo "🚀 Starting server..."
exec "$SERVER_BIN"
