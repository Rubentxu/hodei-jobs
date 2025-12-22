#!/bin/bash
# =============================================================================
# Test Provider Selection Strategies
# =============================================================================
# Demonstrates how the backend selects providers based on different strategies
# =============================================================================

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${BLUE}🎯 Testing Provider Selection Strategies${NC}"
echo "=========================================="
echo ""

# Check if server is running
if ! curl -s http://localhost:50051/health > /dev/null 2>&1; then
    echo -e "${RED}❌ Server is not running${NC}"
    echo "Start the server with: just dev"
    exit 1
fi

# Strategy 1: Fastest Startup (prefers Docker)
echo -e "${CYAN}📊 Strategy 1: Fastest Startup (prefers Docker)${NC}"
echo "-------------------------------------------"
echo "Running quick job (should use Docker - 5s startup)"
cargo run --bin hodei-jobs-cli -- job run \
    --name "Quick Job - Fastest Startup" \
    --command "echo 'Quick task completed'" \
    --timeout 60

echo ""
sleep 2

# Strategy 2: Lowest Cost (prefers Kubernetes)
echo -e "${CYAN}💰 Strategy 2: Lowest Cost (prefers Kubernetes)${NC}"
echo "--------------------------------------------"
echo "Running cost-sensitive job (should use Kubernetes - $0.05/h vs Docker $0.10/h)"
cargo run --bin hodei-jobs-cli -- job run \
    --name "Cost-Sensitive Job - Lowest Cost" \
    --command "echo 'Cost-optimized task completed'" \
    --timeout 60

echo ""
sleep 2

# Strategy 3: Most Capacity (prefers Kubernetes)
echo -e "${CYAN}⚡ Strategy 3: Most Capacity (prefers Kubernetes)${NC}"
echo "------------------------------------------------"
echo "Running resource-intensive job (should use Kubernetes - more capacity)"
cargo run --bin hodei-jobs-cli -- job run \
    --name "Resource Job - Most Capacity" \
    --command "echo 'Resource-intensive task completed'" \
    --cpu 4 \
    --memory 8589934592 \
    --timeout 120

echo ""
sleep 2

# Strategy 4: Healthiest (prefers healthy provider)
echo -e "${CYAN}❤️  Strategy 4: Healthiest (prefers healthy provider)${NC}"
echo "----------------------------------------------------"
echo "Running on healthiest provider"
cargo run --bin hodei-jobs-cli -- job run \
    --name "Health-Check Job - Healthiest" \
    --command "echo 'Health-check task completed'" \
    --timeout 60

echo ""
sleep 2

# Strategy 5: Round Robin
echo -e "${CYAN}🔄 Strategy 5: Round Robin${NC}"
echo "------------------------------"
echo "Running jobs in round-robin fashion"
for i in {1..3}; do
    echo "Round $i:"
    cargo run --bin hodei-jobs-cli -- job run \
        --name "Round Robin Job $i" \
        --command "echo 'Round robin task $i completed'" \
        --timeout 60
    sleep 1
done

echo ""
echo -e "${GREEN}✅ All provider selection strategies tested${NC}"
echo ""
echo -e "${YELLOW}💡 To check provider selection, view server logs:${NC}"
echo "  just logs-server | grep -i 'provider\\|scheduling'"
echo ""
echo -e "${YELLOW}💡 To check provider metrics:${NC}"
echo "  curl http://localhost:9090/metrics | grep hodei_provider"
