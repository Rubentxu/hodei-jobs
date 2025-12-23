#!/bin/bash
# EPIC-21 Provider Selection Verification
# Verifies that jobs execute on the correct provider (Kubernetes vs Docker)

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

NAMESPACE="${HODEI_K8S_NAMESPACE:-hodei-jobs-workers}"
OUTPUT_DIR="/tmp/epic21-provider-$(date +%Y%m%d-%H%M%S)"

mkdir -p "$OUTPUT_DIR"

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║      EPIC-21 Provider Selection Verification                 ║${NC}"
echo -e "${BLUE}║  Kubernetes vs Docker Provider Selection Check               ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""

print_section() {
    echo ""
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${YELLOW}▶ $1${NC}"
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

print_section "1. Kubernetes Provider Status"

echo "🔍 Kubernetes cluster connectivity:"
if kubectl cluster-info &>/dev/null; then
    echo -e "${GREEN}✓${NC} Kubernetes cluster is accessible"
    kubectl cluster-info | head -2
else
    echo -e "${RED}✗${NC} Cannot connect to Kubernetes cluster"
    exit 1
fi

echo ""
echo "🔍 Namespace '$NAMESPACE' status:"
if kubectl get namespace "$NAMESPACE" &>/dev/null; then
    echo -e "${GREEN}✓${NC} Namespace '$NAMESPACE' exists"
else
    echo -e "${YELLOW}⚠${NC} Namespace '$NAMESPACE' does not exist"
    echo "  You can create it with: kubectl create namespace $NAMESPACE"
fi

echo ""
echo "🔍 Worker pods in namespace:"
kubectl get pods -n "$NAMESPACE" --no-headers 2>/dev/null | wc -l | xargs -I {} echo "  Total pods: {}"

echo ""
echo "📊 Recent Kubernetes jobs (last 5):"
kubectl get pods -n "$NAMESPACE" -o json 2>/dev/null | jq -r '.items | sort_by(.metadata.creationTimestamp) | reverse | .[0:5] | .[] | "\(.metadata.name): \(.status.phase)"' 2>/dev/null || echo "  ⚠️  No pods or jq not available"

save_output "k8s-recent-pods" kubectl get pods -n "$NAMESPACE" -o json 2>/dev/null | jq -r '.items | sort_by(.metadata.creationTimestamp) | reverse | .[0:5] | .[] | "\(.metadata.name): \(.status.phase)"' 2>/dev/null || echo "No pods"

print_section "2. Docker Provider Status"

echo "🔍 Docker daemon connectivity:"
if docker info &>/dev/null; then
    echo -e "${GREEN}✓${NC} Docker daemon is running"
    docker version --format 'Server: {{.Server.Version}}' 2>/dev/null || true
else
    echo -e "${RED}✗${NC} Cannot connect to Docker daemon"
    exit 1
fi

echo ""
echo "🔍 Worker containers:"
WORKER_COUNT=$(docker ps -a --filter "label=hodei-jobs-worker" --format "{{.ID}}" 2>/dev/null | wc -l)
echo "  Total worker containers: $WORKER_COUNT"

echo ""
RUNNING_COUNT=$(docker ps --filter "label=hodei-jobs-worker" --filter "status=running" --format "{{.ID}}" 2>/dev/null | wc -l)
echo "  Running: $RUNNING_COUNT"

echo ""
EXITED_COUNT=$(docker ps -a --filter "label=hodei-jobs-worker" --filter "status=exited" --format "{{.ID}}" 2>/dev/null | wc -l)
echo "  Exited (ephemeral termination check): $EXITED_COUNT"

if [[ $EXITED_COUNT -gt 0 ]]; then
    echo ""
    echo "📊 Recently terminated workers (ephemeral behavior):"
    docker ps -a --filter "label=hodei-jobs-worker" --filter "status=exited" --format "{{.Names}}:{{.Status}}:{{.FinishedAt}}" 2>/dev/null | head -5
fi

print_section "3. Provider-Specific Worker Analysis"

echo "🔍 Kubernetes worker details:"
kubectl get pods -n "$NAMESPACE" -o custom-columns=NAME:.metadata.name,STATUS:.status.phase,NODE:.spec.nodeName,AGE:.metadata.creationTimestamp 2>/dev/null | head -10 || echo "  ⚠️  No pods found"

echo ""
echo "🔍 Docker worker labels (provider selection check):"
docker inspect $(docker ps -aq --filter "label=hodei-jobs-worker" 2>/dev/null | head -3) --format '{{.Name}}: {{range $k,$v := .Config.Labels}}{{$k}}={{$v}} {{end}}' 2>/dev/null | grep -E "provider|job" | head -5 || echo "  ⚠️  No labels found"

print_section "4. Database Provider Correlation (Optional)"

if [[ -n "$DATABASE_URL" ]] || [[ -n "$SERVER_DATABASE_URL" ]]; then
    echo "🔍 Querying database for provider correlation:"
    echo ""
    echo "📊 Workers by provider type:"
    psql "$DATABASE_URL" -c "SELECT provider_type, COUNT(*) as count FROM workers GROUP BY provider_type ORDER BY provider_type;" 2>/dev/null || echo "  ❌ Database query failed"

    echo ""
    echo "📊 Recent jobs with provider assignment:"
    psql "$DATABASE_URL" -c "SELECT id, state, provider_id::text FROM jobs ORDER BY created_at DESC LIMIT 5;" 2>/dev/null || echo "  ❌ Database query failed"
else
    echo "⚠️  Database URL not set. Skipping database queries."
fi

print_section "5. EPIC-21 Provider Selection Verification"

echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📋 Provider Selection Compliance Check:${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Expected Behavior (EPIC-21):"
echo ""
echo "1. Jobs with --provider kubernetes:"
echo "   ✓ Should create Kubernetes pods"
echo "   ✓ Should appear in namespace: $NAMESPACE"
echo "   ✓ Should NOT create Docker containers"
echo ""
echo "2. Jobs with --provider docker:"
echo "   ✓ Should create Docker containers"
echo "   ✓ Should have label: $DOCKER_LABEL"
echo "   ✓ Should NOT create Kubernetes pods"
echo ""
echo "3. Ephemeral Workers:"
echo "   ✓ Workers terminate after job completion"
echo "   ✓ No worker reuse between jobs"
echo "   ✓ Each job creates a new worker"

echo ""
echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${YELLOW}💡 Testing Provider Selection:${NC}"
echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "To test provider selection manually:"
echo ""
echo "1. Start monitoring in another terminal:"
echo "   ${CYAN}./scripts/watch-epic21-workers.sh${NC}"
echo ""
echo "2. Run a Kubernetes job:"
echo "   ${CYAN}just job-k8s-hello${NC}"
echo "   # Should create a K8s pod, not a Docker container"
echo ""
echo "3. Run a Docker job:"
echo "   ${CYAN}just job-docker-hello${NC}"
echo "   # Should create a Docker container, not a K8s pod"
echo ""
echo "4. Verify provider selection:"
echo "   ${CYAN}./scripts/verify-epic21-with-cli.sh${NC}"

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}📁 Verification data saved to: $OUTPUT_DIR${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

ls -lh "$OUTPUT_DIR" 2>/dev/null | grep -v "^total" | awk '{print "  - " $9 " (" $5 ")"}' | head -10
