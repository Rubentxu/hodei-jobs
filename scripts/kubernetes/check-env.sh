#!/bin/bash
# Hodei Kubernetes Environment Checker
# Verifies that all requirements for Kubernetes provider are met

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ERRORS=0
WARNINGS=0

check_pass() {
    echo -e "${GREEN}✓${NC} $1"
}

check_fail() {
    echo -e "${RED}✗${NC} $1"
    ((ERRORS++))
}

check_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
    ((WARNINGS++))
}

echo "=== Hodei Kubernetes Environment Check ==="
echo

# Check kubectl
echo "Checking kubectl..."
if command -v kubectl &> /dev/null; then
    KUBECTL_VERSION=$(kubectl version --client -o json 2>/dev/null | jq -r '.clientVersion.gitVersion' 2>/dev/null || echo "unknown")
    check_pass "kubectl installed: ${KUBECTL_VERSION}"
else
    check_fail "kubectl is not installed"
    echo "    Install from: https://kubernetes.io/docs/tasks/tools/"
fi

# Check cluster connectivity
echo
echo "Checking cluster connectivity..."
if kubectl cluster-info &> /dev/null; then
    CONTEXT=$(kubectl config current-context 2>/dev/null || echo "unknown")
    check_pass "Connected to cluster: ${CONTEXT}"
else
    check_fail "Cannot connect to Kubernetes cluster"
    echo "    Check your kubeconfig: kubectl config view"
fi

# Check namespace
echo
echo "Checking namespaces..."
K8S_NAMESPACE="${HODEI_K8S_NAMESPACE:-hodei-workers}"
if kubectl get namespace "${K8S_NAMESPACE}" &> /dev/null; then
    check_pass "Namespace exists: ${K8S_NAMESPACE}"
else
    check_warn "Namespace not found: ${K8S_NAMESPACE}"
    echo "    Create with: kubectl create namespace ${K8S_NAMESPACE}"
    echo "    Or run: ./deploy/kubernetes/install.sh"
fi

# Check RBAC
echo
echo "Checking RBAC..."
if kubectl get serviceaccount hodei-worker -n "${K8S_NAMESPACE}" &> /dev/null; then
    check_pass "ServiceAccount exists: hodei-worker"
else
    check_warn "ServiceAccount not found: hodei-worker"
    echo "    Apply RBAC: kubectl apply -f deploy/kubernetes/rbac.yaml"
fi

# Check permissions
echo
echo "Checking permissions..."
CAN_CREATE_PODS=$(kubectl auth can-i create pods -n "${K8S_NAMESPACE}" 2>/dev/null || echo "no")
CAN_DELETE_PODS=$(kubectl auth can-i delete pods -n "${K8S_NAMESPACE}" 2>/dev/null || echo "no")
CAN_GET_LOGS=$(kubectl auth can-i get pods/log -n "${K8S_NAMESPACE}" 2>/dev/null || echo "no")

if [[ "${CAN_CREATE_PODS}" == "yes" ]]; then
    check_pass "Can create pods in ${K8S_NAMESPACE}"
else
    check_fail "Cannot create pods in ${K8S_NAMESPACE}"
fi

if [[ "${CAN_DELETE_PODS}" == "yes" ]]; then
    check_pass "Can delete pods in ${K8S_NAMESPACE}"
else
    check_fail "Cannot delete pods in ${K8S_NAMESPACE}"
fi

if [[ "${CAN_GET_LOGS}" == "yes" ]]; then
    check_pass "Can get pod logs in ${K8S_NAMESPACE}"
else
    check_warn "Cannot get pod logs in ${K8S_NAMESPACE}"
fi

# Check worker image
echo
echo "Checking worker image..."
WORKER_IMAGE="${HODEI_WORKER_IMAGE:-hodei-worker:latest}"
# Try to pull image info (this may fail for private registries)
if kubectl run test-image-check --image="${WORKER_IMAGE}" --restart=Never --dry-run=client -o yaml &> /dev/null; then
    check_pass "Worker image configured: ${WORKER_IMAGE}"
else
    check_warn "Could not verify worker image: ${WORKER_IMAGE}"
fi

# Check network policies
echo
echo "Checking network policies..."
if kubectl get networkpolicy -n "${K8S_NAMESPACE}" &> /dev/null; then
    NP_COUNT=$(kubectl get networkpolicy -n "${K8S_NAMESPACE}" -o json 2>/dev/null | jq '.items | length')
    if [[ "${NP_COUNT}" -gt 0 ]]; then
        check_pass "Network policies configured: ${NP_COUNT} policies"
    else
        check_warn "No network policies in ${K8S_NAMESPACE}"
    fi
else
    check_warn "Could not check network policies"
fi

# Check resource quotas
echo
echo "Checking resource quotas..."
if kubectl get resourcequota -n "${K8S_NAMESPACE}" &> /dev/null; then
    RQ_COUNT=$(kubectl get resourcequota -n "${K8S_NAMESPACE}" -o json 2>/dev/null | jq '.items | length')
    if [[ "${RQ_COUNT}" -gt 0 ]]; then
        check_pass "Resource quotas configured: ${RQ_COUNT} quotas"
    else
        check_warn "No resource quotas in ${K8S_NAMESPACE} (recommended for production)"
    fi
else
    check_warn "Could not check resource quotas"
fi

# Check cluster resources
echo
echo "Checking cluster resources..."
NODE_COUNT=$(kubectl get nodes -o json 2>/dev/null | jq '.items | length' || echo "0")
if [[ "${NODE_COUNT}" -gt 0 ]]; then
    check_pass "Cluster has ${NODE_COUNT} node(s)"
    
    # Check node resources
    TOTAL_CPU=$(kubectl get nodes -o json 2>/dev/null | jq '[.items[].status.capacity.cpu | tonumber] | add' || echo "0")
    TOTAL_MEM=$(kubectl get nodes -o json 2>/dev/null | jq '[.items[].status.capacity.memory | gsub("Ki";"") | tonumber] | add / 1024 / 1024' || echo "0")
    
    check_pass "Total cluster CPU: ${TOTAL_CPU} cores"
    check_pass "Total cluster memory: ${TOTAL_MEM}GB"
else
    check_fail "No nodes found in cluster"
fi

# Summary
echo
echo "=== Summary ==="
if [[ ${ERRORS} -eq 0 ]] && [[ ${WARNINGS} -eq 0 ]]; then
    echo -e "${GREEN}All checks passed!${NC}"
    echo "Kubernetes provider is ready to use."
elif [[ ${ERRORS} -eq 0 ]]; then
    echo -e "${YELLOW}${WARNINGS} warning(s), 0 errors${NC}"
    echo "Kubernetes provider should work but may have limited functionality."
else
    echo -e "${RED}${ERRORS} error(s), ${WARNINGS} warning(s)${NC}"
    echo "Please fix the errors before using Kubernetes provider."
fi

echo
echo "Environment variables for Hodei:"
echo "  HODEI_K8S_ENABLED=1"
echo "  HODEI_K8S_NAMESPACE=${K8S_NAMESPACE}"
echo "  HODEI_WORKER_IMAGE=${WORKER_IMAGE}"

exit ${ERRORS}
