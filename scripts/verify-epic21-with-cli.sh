#!/bin/bash
# EPIC-21 Verification Script using kubectl and Docker CLI
# Verifies ephemeral workers behavior with provider-specific selection

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
NAMESPACE="${HODEI_K8S_NAMESPACE:-hodei-jobs-workers}"
DOCKER_LABEL="hodei-jobs-worker"
OUTPUT_DIR="/tmp/epic21-verification-$(date +%Y%m%d-%H%M%S)"

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║        EPIC-21 Verification: Ephemeral Workers & Provider     ║${NC}"
echo -e "${BLUE}║                  Selection Verification                      ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Function to print section headers
print_section() {
    echo ""
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${YELLOW}▶ $1${NC}"
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Function to save output to file
save_output() {
    local name="$1"
    shift
    echo "$@" > "$OUTPUT_DIR/$name.txt"
}

print_section "1. Kubernetes Pods Verification (K8s Workers)"

echo "🔍 Checking Kubernetes pods in namespace: $NAMESPACE"
echo ""

# Get all pods related to hodei-jobs
echo "📋 All pods in $NAMESPACE namespace:"
kubectl get pods -n "$NAMESPACE" -o wide 2>/dev/null || echo "❌ Failed to get pods"

save_output "k8s-pods-all" kubectl get pods -n "$NAMESPACE" -o wide

echo ""
echo "🔍 Pods with 'hodei-jobs-worker' label:"
kubectl get pods -n "$NAMESPACE" -l app=hodei-jobs-worker -o wide 2>/dev/null || echo "❌ No pods found or namespace not accessible"

save_output "k8s-pods-worker" kubectl get pods -n "$NAMESPACE" -l app=hodei-jobs-worker -o wide

echo ""
echo "📊 Pod status summary:"
kubectl get pods -n "$NAMESPACE" --no-headers 2>/dev/null | awk '{print $1, $3, $5}' | sort || echo "❌ Failed to get pod status"

save_output "k8s-pod-status" kubectl get pods -n "$NAMESPACE" --no-headers 2>/dev/null | awk '{print $1, $3, $5}'

# Check for ephemeral behavior
print_section "2. Kubernetes Ephemeral Behavior Analysis"

echo "⏰ Checking pod age and lifecycle:"
kubectl get pods -n "$NAMESPACE" -o custom-columns=NAME:.metadata.name,STATUS:.status.phase,AGE:.metadata.creationTimestamp,START:.status.startTime 2>/dev/null || echo "❌ Failed to get pod details"

save_output "k8s-pod-lifecycle" kubectl get pods -n "$NAMESPACE" -o custom-columns=NAME:.metadata.name,STATUS:.status.phase,AGE:.metadata.creationTimestamp,START:.status.startTime 2>/dev/null

echo ""
echo "🔍 Looking for completed/failed pods (ephemeral termination check):"
kubectl get pods -n "$NAMESPACE" --field-selector=status.phase=Succeeded,status.phase=Failed -o json 2>/dev/null | jq -r '.items[] | "\(.metadata.name): \(.status.phase) at \(.status.completionTime // .metadata.creationTimestamp)"' 2>/dev/null || echo "⚠️  No completed pods found or jq not available"

save_output "k8s-completed-pods" kubectl get pods -n "$NAMESPACE" --field-selector=status.phase=Succeeded,status.phase=Failed -o json 2>/dev/null | jq -r '.items[] | "\(.metadata.name): \(.status.phase)"' 2>/dev/null || echo "No completed pods"

print_section "3. Docker Containers Verification (Docker Workers)"

echo "🔍 Checking Docker containers with label: $DOCKER_LABEL"
echo ""

echo "📋 Running containers:"
docker ps --filter "label=$DOCKER_LABEL" --format "table {{.ID}}\t{{.Image}}\t{{.Status}}\t{{.Names}}" 2>/dev/null || echo "❌ Docker not available or no containers found"

save_output "docker-running" docker ps --filter "label=$DOCKER_LABEL" --format "table {{.ID}}\t{{.Image}}\t{{.Status}}\t{{.Names}}" 2>/dev/null || echo "Docker not available"

echo ""
echo "📋 All containers (including stopped):"
docker ps -a --filter "label=$DOCKER_LABEL" --format "table {{.ID}}\t{{.Image}}\t{{.Status}}\t{{.Names}}\t{{.CreatedAt}}" 2>/dev/null || echo "❌ Docker not available"

save_output "docker-all" docker ps -a --filter "label=$DOCKER_LABEL" --format "table {{.ID}}\t{{.Image}}\t{{.Status}}\t{{.Names}}\t{{.CreatedAt}}" 2>/dev/null || echo "Docker not available"

# Check for ephemeral behavior in Docker
print_section "4. Docker Ephemeral Behavior Analysis"

echo "⏰ Checking container lifecycle:"
docker ps -a --filter "label=$DOCKER_LABEL" --format "{{.ID}}:{{.Status}}:{{.Names}}" 2>/dev/null | while read line; do
    CONTAINER_ID=$(echo "$line" | cut -d: -f1)
    STATUS=$(echo "$line" | cut -d: -f2)
    NAME=$(echo "$line" | cut -d: -f3-)

    if [[ "$STATUS" == "Exited" ]] || [[ "$STATUS" == "Dead" ]]; then
        echo "  ✅ Ephemeral behavior confirmed: $NAME ($CONTAINER_ID) - Status: $STATUS"
        docker inspect "$CONTAINER_ID" --format "{{.State.FinishedAt}}" 2>/dev/null | head -1
    fi
done 2>/dev/null || echo "⚠️  No exited containers found"

save_output "docker-exited" docker ps -a --filter "label=$DOCKER_LABEL" --filter "status=exited" --format "{{.ID}}:{{.Names}}:{{.Status}}:{{.FinishedAt}}" 2>/dev/null || echo "No exited containers"

print_section "5. Provider Selection Verification"

echo "🔍 Checking provider-specific resources:"
echo ""

echo "📋 Kubernetes nodes used by pods:"
kubectl get pods -n "$NAMESPACE" -o json 2>/dev/null | jq -r '.items[] | "\(.metadata.name): \(.spec.nodeName // "unassigned")"' 2>/dev/null || echo "⚠️  Unable to query pod node assignments"

echo ""
echo "📋 Docker containers with provider annotations:"
docker inspect $(docker ps -aq --filter "label=$DOCKER_LABEL") --format '{{range .}}{{.Name}}: {{.Config.Labels}}{{"\n"}}{{end}}' 2>/dev/null | grep -E "provider|provider_type" || echo "⚠️  No provider annotations found"

save_output "docker-provider-info" docker inspect $(docker ps -aq --filter "label=$DOCKER_LABEL") --format '{{range .}}{{.Name}}: {{.Config.Labels}}{{"\n"}}{{end}}' 2>/dev/null || echo "No provider info"

print_section "6. Database State Verification"

echo "🔍 Checking database for worker lifecycle (requires server access):"
echo ""

# Try to query database if credentials are available
if [[ -n "$SERVER_DATABASE_URL" ]] || [[ -n "$DATABASE_URL" ]]; then
    echo "📊 Workers table (current state):"
    psql "$DATABASE_URL" -c "SELECT id, provider_type, state, current_job_id, created_at FROM workers ORDER BY created_at DESC LIMIT 20;" 2>/dev/null || echo "❌ Database query failed"

    save_output "db-workers" psql "$DATABASE_URL" -c "SELECT id, provider_type, state, current_job_id FROM workers ORDER BY created_at DESC LIMIT 20;" 2>/dev/null || echo "DB query failed"

    echo ""
    echo "📊 Recent jobs (ephemeral worker correlation):"
    psql "$DATABASE_URL" -c "SELECT id, state, provider_id, created_at FROM jobs ORDER BY created_at DESC LIMIT 10;" 2>/dev/null || echo "❌ Database query failed"

    save_output "db-jobs" psql "$DATABASE_URL" -c "SELECT id, state, provider_id FROM jobs ORDER BY created_at DESC LIMIT 10;" 2>/dev/null || echo "DB query failed"
else
    echo "⚠️  Database URL not set. Set DATABASE_URL or SERVER_DATABASE_URL to query database."
fi

print_section "7. Log Analysis"

echo "🔍 Extracting recent logs from Kubernetes:"
echo ""

# Get recent pod logs (last 50 lines)
kubectl get pods -n "$NAMESPACE" -o name 2>/dev/null | head -5 | while read pod; do
    echo "📄 Logs from $pod:"
    kubectl logs -n "$NAMESPACE" "$pod" --tail=20 2>/dev/null || echo "  ❌ Unable to get logs"
    echo ""
done

save_output "k8s-logs" kubectl logs -n "$NAMESPACE" $(kubectl get pods -n "$NAMESPACE" -o name 2>/dev/null | head -1 | sed 's/pod\///') --tail=50 2>/dev/null || echo "No logs available"

echo ""
echo "🔍 Recent Docker container logs:"
docker ps --filter "label=$DOCKER_LABEL" -q 2>/dev/null | head -3 | while read cid; do
    echo "📄 Logs from $cid:"
    docker logs --tail=20 "$cid" 2>/dev/null || echo "  ❌ Unable to get logs"
    echo ""
done

print_section "8. EPIC-21 Compliance Summary"

echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📋 EPIC-21 Compliance Checklist:${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "✅ Workers are ephemeral (1 job per worker):"
echo "   - Check K8s pods: Should show Succeeded/Failed pods are terminated"
echo "   - Check Docker containers: Exited/Dead containers are removed or not reused"
echo ""
echo "✅ Provider selection is enforced:"
echo "   - Jobs with --provider kubernetes should create K8s pods"
echo "   - Jobs with --provider docker should create Docker containers"
echo ""
echo "✅ Workers are terminated after job completion:"
echo "   - Check K8s: Pods should transition to Completed state"
echo "   - Check Docker: Containers should exit and not be reused"
echo ""
echo "✅ No worker reuse between jobs:"
echo "   - Each job creates a new worker"
echo "   - Workers are not assigned to multiple jobs"

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}📁 Verification artifacts saved to: $OUTPUT_DIR${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "📄 Files generated:"
ls -lh "$OUTPUT_DIR" | grep -v "^total" | awk '{print "  - " $9 " (" $5 ")"}'

echo ""
echo -e "${YELLOW}💡 Next Steps:${NC}"
echo "   1. Review the generated artifacts in $OUTPUT_DIR"
echo "   2. Check for any workers in unexpected states"
echo "   3. Verify provider selection worked correctly"
echo "   4. Confirm ephemeral behavior (no worker reuse)"
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
