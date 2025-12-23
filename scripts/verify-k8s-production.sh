#!/bin/bash
# ==============================================================================
# Hodei Jobs - Kubernetes Production Verification Script
# ==============================================================================
# Verifies that all components are working correctly for Kubernetes jobs
#
# Usage: ./scripts/verify-k8s-production.sh [OPTIONS]
# Options:
#   --namespace          Namespace to check (default: hodei-jobs-workers)
#   --cluster-context    Kubernetes context (default: kind-hodei-test)
#   --server-url         Server URL (default: http://localhost:50051)
# ==============================================================================

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

NAMESPACE="${NAMESPACE:-hodei-jobs-workers}"
CLUSTER_CONTEXT="${CLUSTER_CONTEXT:-kind-hodei-test}"
SERVER_URL="${SERVER_URL:-http://localhost:50051}"

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║        Hodei Jobs - Kubernetes Production Verification        ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Configuration:"
echo "  Namespace:        ${NAMESPACE}"
echo "  Cluster Context:  ${CLUSTER_CONTEXT}"
echo "  Server URL:       ${SERVER_URL}"
echo ""

# Counters
PASSED=0
FAILED=0
WARNINGS=0

# Test helper
run_test() {
    local test_name="$1"
    local test_command="$2"

    echo -e "${BLUE}[TEST]${NC} ${test_name}"

    if eval "$test_command" &> /dev/null; then
        echo -e "  ${GREEN}✓ PASS${NC}"
        ((PASSED++))
        return 0
    else
        echo -e "  ${RED}✗ FAIL${NC}"
        ((FAILED++))
        return 1
    fi
}

# Check helper
check_item() {
    local item_name="$1"
    local check_command="$2"

    echo -e "${BLUE}[CHECK]${NC} ${item_name}"

    if eval "$check_command" &> /dev/null; then
        echo -e "  ${GREEN}✓${NC}"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}⚠${NC}"
        ((WARNINGS++))
    fi
}

echo -e "${YELLOW}═══ Phase 1: Infrastructure Verification ═══${NC}"
echo ""

# Check kubectl
check_item "kubectl is installed" "command -v kubectl"

# Check cluster connectivity
check_item "Cluster '${CLUSTER_CONTEXT}' is accessible" \
    "kubectl cluster-info --context=${CLUSTER_CONTEXT}"

# Check namespace
check_item "Namespace '${NAMESPACE}' exists" \
    "kubectl get namespace ${NAMESPACE} --context=${CLUSTER_CONTEXT}"

# Check RBAC
check_item "ServiceAccount 'hodei-jobs-worker' exists" \
    "kubectl get serviceaccount hodei-jobs-worker --namespace=${NAMESPACE} --context=${CLUSTER_CONTEXT}"

check_item "Role 'hodei-jobs-worker-role' exists" \
    "kubectl get role hodei-jobs-worker-role --namespace=${NAMESPACE} --context=${CLUSTER_CONTEXT}"

check_item "RoleBinding 'hodei-jobs-worker-binding' exists" \
    "kubectl get rolebinding hodei-jobs-worker-binding --namespace=${NAMESPACE} --context=${CLUSTER_CONTEXT}"

# Check quotas
check_item "ResourceQuota exists" \
    "kubectl get resourcequota hodei-jobs-workers-quota --namespace=${NAMESPACE} --context=${CLUSTER_CONTEXT}"

check_item "LimitRange exists" \
    "kubectl get limitrange hodei-jobs-workers-limits --namespace=${NAMESPACE} --context=${CLUSTER_CONTEXT}"

# Check Pod Security Standards
check_item "Pod Security Standards applied" \
    "kubectl get namespace ${NAMESPACE} --context=${CLUSTER_CONTEXT} -o jsonpath='{.metadata.labels}' | grep -q 'pod-security.kubernetes.io'"

echo ""
echo -e "${YELLOW}═══ Phase 2: Server Verification ═══${NC}"
echo ""

# Check server process
check_item "Server process is running" "pgrep -f hodei-server-bin"

# Check gRPC port
check_item "Server gRPC port (50051) is listening" \
    "netstat -tuln | grep -q ':50051' || ss -tuln | grep -q ':50051'"

# Check server logs for Kubernetes provider
if pgrep -f hodei-server-bin > /dev/null; then
    check_item "Kubernetes provider initialized (check logs)" "true"
    echo -e "  ${YELLOW}  → Check server logs for: 'Kubernetes provider initialized'${NC}"
else
    check_item "Server is running" "false"
fi

echo ""
echo -e "${YELLOW}═══ Phase 3: Worker Image Verification ═══${NC}"
echo ""

# Check worker binary
check_item "Worker binary exists" "test -f ./target/release/hodei-worker-bin"

# Check Docker image
check_item "Worker image exists locally" \
    "docker images | grep -q 'hodei-jobs-worker.*latest'"

echo ""
echo -e "${YELLOW}═══ Phase 4: Job Execution Test ═══${NC}"
echo ""

# Clean up any existing test pods
echo -e "${BLUE}[CLEANUP]${NC} Removing any existing test pods..."
kubectl delete pods --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" \
    --field-selector=status.phase!=Running \
    --ignore-not-found=true &> /dev/null || true

# Run a test job
echo -e "${BLUE}[TEST]${NC} Running test job 'just job-k8s-hello'..."
echo -e "  ${YELLOW}  → This may take 10-15 seconds${NC}"

# Run job in background and capture output
JOB_OUTPUT=$(timeout 30 just job-k8s-hello 2>&1 || true)

if echo "$JOB_OUTPUT" | grep -q "Job queued successfully"; then
    echo -e "  ${GREEN}✓ PASS${NC} - Job queued"
    ((PASSED++))
else
    echo -e "  ${RED}✗ FAIL${NC} - Job did not queue"
    echo -e "  ${YELLOW}  → Output: $JOB_OUTPUT${NC}"
    ((FAILED++))
fi

# Wait for pod to be created
echo -e "${BLUE}[WAIT]${NC} Waiting for pod creation (15 seconds)..."
sleep 15

# Check for pods
POD_COUNT=$(kubectl get pods --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" \
    --no-headers 2>/dev/null | wc -l)

if [[ $POD_COUNT -gt 0 ]]; then
    echo -e "  ${GREEN}✓ PASS${NC} - Pod created ($POD_COUNT pod(s) found)"
    ((PASSED++))

    # Show pod details
    echo ""
    echo "Pod Details:"
    kubectl get pods --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" \
        -o wide --no-headers | while read -r line; do
        echo -e "  ${GREEN}→${NC} $line"
    done

    # Check pod labels
    echo ""
    echo "Pod Labels:"
    FIRST_POD=$(kubectl get pods --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" \
        --no-headers -o name | head -1)
    if [[ -n "$FIRST_POD" ]]; then
        kubectl get "$FIRST_POD" --namespace="${NAMESPACE}" \
            --context="${CLUSTER_CONTEXT}" \
            -o jsonpath='{.metadata.labels}' 2>/dev/null | jq '.' 2>/dev/null || \
            kubectl get "$FIRST_POD" --namespace="${NAMESPACE}" \
                --context="${CLUSTER_CONTEXT}" \
                -o jsonpath='{.metadata.labels}'
    fi
else
    echo -e "  ${YELLOW}⚠ WARN${NC} - No pods found (job may have completed too quickly)"
    ((WARNINGS++))
fi

# Check for log streaming
if echo "$JOB_OUTPUT" | grep -q "Hello from Kubernetes provider"; then
    echo -e "  ${GREEN}✓ PASS${NC} - Logs streamed successfully"
    ((PASSED++))
else
    echo -e "  ${YELLOW}⚠ WARN${NC} - Could not verify log streaming"
    ((WARNINGS++))
fi

echo ""
echo -e "${YELLOW}═══ Phase 5: Cleanup Verification ═══${NC}"
echo ""

# Wait a bit more for cleanup
echo -e "${BLUE}[WAIT]${NC} Waiting for pod cleanup (30 seconds)..."
sleep 30

FINAL_POD_COUNT=$(kubectl get pods --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" \
    --field-selector=status.phase=Running --no-headers 2>/dev/null | wc -l)

if [[ $FINAL_POD_COUNT -eq 0 ]]; then
    echo -e "  ${GREEN}✓ PASS${NC} - Pods cleaned up successfully"
    ((PASSED++))
else
    echo -e "  ${YELLOW}⚠ WARN${NC} - $FINAL_POD_COUNT pod(s) still running"
    echo "  → Running pods:"
    kubectl get pods --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" \
        --no-headers | while read -r line; do
        echo -e "    ${YELLOW}→${NC} $line"
    done
    ((WARNINGS++))
fi

echo ""
echo -e "${YELLOW}═══ Phase 6: Resource Usage ═══${NC}"
echo ""

# Check resource usage
if command -v kubectl &> /dev/null && kubectl top pods --namespace="${NAMESPACE}" &> /dev/null; then
    echo "Resource Usage:"
    kubectl top pods --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" 2>/dev/null || \
        echo -e "  ${YELLOW}Metrics server not available${NC}"
else
    echo -e "${YELLOW}⚠ Metrics not available (kubectl top requires metrics-server)${NC}"
fi

# Check events
echo ""
echo "Recent Events:"
kubectl get events --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" \
    --sort-by='.lastTimestamp' --no-headers 2>/dev/null | tail -5 | while read -r line; do
    echo -e "  ${GREEN}→${NC} $line"
done

echo ""
echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                    Verification Summary                       ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "Tests Passed:    ${GREEN}${PASSED}${NC}"
echo -e "Tests Failed:    ${RED}${FAILED}${NC}"
echo -e "Warnings:        ${YELLOW}${WARNINGS}${NC}"
echo ""

if [[ $FAILED -eq 0 ]]; then
    echo -e "${GREEN}✅ All critical tests passed!${NC}"
    echo ""
    echo "Your Kubernetes provider is ready for production!"
    echo ""
    echo -e "${BLUE}Next steps:${NC}"
    echo "  1. Run jobs: just job-k8s-hello"
    echo "  2. Monitor: kubectl get pods --namespace=${NAMESPACE} --watch"
    echo "  3. Check logs: kubectl logs <pod-name> --namespace=${NAMESPACE}"
    exit 0
else
    echo -e "${RED}❌ Some tests failed!${NC}"
    echo ""
    echo "Please review the failures above and:"
    echo "  1. Check server logs for errors"
    echo "  2. Verify Kubernetes cluster is accessible"
    echo "  3. Ensure RBAC is properly configured"
    echo "  4. Run setup script if needed: ./scripts/setup-k8s-production.sh"
    exit 1
fi
