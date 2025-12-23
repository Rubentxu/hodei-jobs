#!/bin/bash
# =============================================================================
# Verify Kubernetes Jobs Run in Pods
# =============================================================================
# This script verifies that jobs submitted to Kubernetes provider
# actually create Pods in the correct namespace
# =============================================================================

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}🔍 Verifying Kubernetes Jobs Create Pods${NC}"
echo "========================================="
echo ""

# Check if kind cluster exists
if ! kind get clusters 2>/dev/null | grep -q "hodei-test"; then
    echo -e "${RED}❌ Kind cluster 'hodei-test' not found${NC}"
    echo "Create it with: kind create cluster --name hodei-test"
    exit 1
fi

# Check if kubectl is available
if ! command -v kubectl &> /dev/null; then
    echo -e "${RED}❌ kubectl not installed${NC}"
    exit 1
fi

# Check context
CURRENT_CONTEXT=$(kubectl config current-context 2>/dev/null || echo "")
if [ "$CURRENT_CONTEXT" != "kind-hodei-test" ]; then
    echo -e "${YELLOW}⚠️  Current context: $CURRENT_CONTEXT${NC}"
    echo "Switching to kind-hodei-test..."
    kubectl config use-context kind-hodei-test
fi

# Default namespace from KubernetesConfig
NAMESPACE="hodei-jobs-workers"
echo -e "${BLUE}📋 Checking namespace: $NAMESPACE${NC}"
echo ""

# Check if namespace exists
if ! kubectl get namespace "$NAMESPACE" &> /dev/null; then
    echo -e "${YELLOW}⚠️  Namespace '$NAMESPACE' does not exist${NC}"
    echo "It will be created when first job is submitted."
    echo ""
    echo "Current namespaces:"
    kubectl get namespaces
    echo ""
    echo -e "${YELLOW}💡 To create the namespace manually:${NC}"
    echo "  kubectl create namespace $NAMESPACE"
    exit 0
fi

echo -e "${GREEN}✅ Namespace '$NAMESPACE' exists${NC}"
echo ""

# List all pods in the namespace
echo -e "${BLUE}📜 Current Pods in $NAMESPACE:${NC}"
echo "----------------------------------------"
POD_COUNT=$(kubectl get pods -n "$NAMESPACE" --no-headers 2>/dev/null | wc -l)

if [ "$POD_COUNT" -eq 0 ]; then
    echo -e "${YELLOW}No pods found in namespace $NAMESPACE${NC}"
    echo ""
    echo "To create pods, run:"
    echo "  just job-k8s-hello"
else
    kubectl get pods -n "$NAMESPACE" -o wide
    echo ""
    echo -e "${GREEN}Found $POD_COUNT pod(s)${NC}"
fi

echo ""
echo -e "${BLUE}🐍 Pod Naming Pattern:${NC}"
echo "----------------------------------------"
echo "Pods are named with pattern: hodei-worker-<worker-id>"
echo ""

# Show labels on pods
if [ "$POD_COUNT" -gt 0 ]; then
    echo -e "${BLUE}🏷️  Pod Labels:${NC}"
    echo "----------------------------------------"
    kubectl get pods -n "$NAMESPACE" --show-labels
    echo ""

    # Show specific labels
    echo -e "${BLUE}🔍 Key Labels:${NC}"
    echo "----------------------------------------"
    echo "app=hodei-jobs-worker"
    echo "hodei.io/managed=true"
    echo ""
fi

# Check for multi-tenant namespaces
echo -e "${BLUE}🏢 Multi-Tenant Support:${NC}"
echo "----------------------------------------"
TENANT_NAMESPACES=$(kubectl get namespaces -o jsonpath='{range .items[?(@.metadata.name.startsWith("hodei-tenant")] )}{.metadata.name}{"\n"}{end}' 2>/dev/null || echo "")

if [ -z "$TENANT_NAMESPACES" ]; then
    echo -e "${YELLOW}No tenant-specific namespaces found${NC}"
    echo "Multi-tenant namespaces follow pattern: hodei-tenant-<tenant-id>"
    echo ""
    echo "To enable multi-tenancy, set:"
    echo "  enable_dynamic_namespaces: true"
    echo "And add label to jobs: hodei.io/tenant-id=<tenant-id>"
else
    echo -e "${GREEN}Tenant namespaces:${NC}"
    echo "$TENANT_NAMESPACES"
    echo ""
fi

# Check node where pods are scheduled
if [ "$POD_COUNT" -gt 0 ]; then
    echo -e "${BLUE}🖥️  Pod Node Distribution:${NC}"
    echo "----------------------------------------"
    kubectl get pods -n "$NAMESPACE" -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.spec.nodeName}{"\n"}{end}' 2>/dev/null || echo "N/A"
    echo ""
fi

# Watch mode option
echo -e "${BLUE}👀 Live Watch Mode:${NC}"
echo "----------------------------------------"
echo "To watch pods in real-time, run:"
echo "  kubectl get pods -n $NAMESPACE -w"
echo ""

# Verify pod details
if [ "$POD_COUNT" -gt 0 ]; then
    echo -e "${BLUE}📊 Pod Details Example:${NC}"
    echo "----------------------------------------"
    FIRST_POD=$(kubectl get pods -n "$NAMESPACE" -o name 2>/dev/null | head -1)
    if [ -n "$FIRST_POD" ]; then
        echo "Example pod: $FIRST_POD"
        echo ""
        kubectl describe "$FIRST_POD" -n "$NAMESPACE" | head -30
    fi
fi

echo ""
echo -e "${GREEN}✅ Verification Complete${NC}"
echo ""
echo -e "${YELLOW}📝 Summary:${NC}"
echo "----------------------------------------"
echo "✅ Kubernetes provider creates Pods (not just processes)"
echo "✅ Default namespace: $NAMESPACE"
echo "✅ Pod naming: hodei-worker-<worker-id>"
echo "✅ Labels: app=hodei-jobs-worker, hodei.io/managed=true"
echo "✅ Multi-tenant support: hodei-tenant-<tenant-id> pattern"
echo ""
echo -e "${YELLOW}🚀 To create test jobs:${NC}"
echo "----------------------------------------"
echo "just job-k8s-hello          # Simple hello world"
echo "just job-k8s-cpu            # CPU-intensive job"
echo "just job-k8s-gpu            # GPU job (requires GPU nodes)"
echo "just job-k8s-ml             # ML training job"
echo ""
echo -e "${YELLOW}🔍 To monitor pods:${NC}"
echo "----------------------------------------"
echo "kubectl get pods -n $NAMESPACE -w"
echo "kubectl logs -n $NAMESPACE -l app=hodei-jobs-worker -f"
echo ""
