#!/bin/bash
set -euo pipefail

echo "🚀 Starting Hello World Job"
echo "==========================="
echo ""
echo "📝 Job Information:"
echo "   - Name: Hello World"
echo "   - Type: Simple Demo"
echo "   - Purpose: Test basic job execution"
echo ""

echo "⏰ Starting execution at $(date)"
echo ""

echo "📊 Step 1/3: Initialization..."
echo "   ✓ Environment variables loaded"
echo "   ✓ Working directory: $(pwd)"
echo "   ✓ User: $(whoami)"
sleep 1

echo ""
echo "📊 Step 2/3: Processing..."
echo "   ✓ Generating sample data..."
for i in {1..5}; do
    echo "   → Processing item $i/5..."
    sleep 0.5
done

echo ""
echo "📊 Step 3/3: Completion..."
echo "   ✓ All items processed successfully"
echo "   ✓ Generating summary report..."
sleep 1

echo ""
echo "✅ Job completed successfully!"
echo "==========================="
echo "⏰ Finished at $(date)"
echo "🎉 Hello from Hodei Job Platform!"
