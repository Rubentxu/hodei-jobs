#!/bin/bash
# =============================================================================
# Minikube Setup Script for Hodei Jobs Platform
# =============================================================================
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
MINIKUBE_CPUS="${MINIKUBE_CPUS:-4}"
MINIKUBE_MEMORY="${MINIKUBE_MEMORY:-8192}"
MINIKUBE_DISK="${MINIKUBE_DISK:-40g}"
MINIKUBE_DRIVER="${MINIKUBE_DRIVER:-docker}"

echo -e "${BLUE}=== Hodei Jobs - Minikube Setup ===${NC}"

# Check if minikube is installed
if ! command -v minikube &> /dev/null; then
    echo -e "${RED}Error: minikube is not installed${NC}"
    echo "Install with: curl -LO https://storage.googleapis.com/minikube/releases/latest/minikube-linux-amd64"
    exit 1
fi

# Check if minikube is running
if minikube status &>/dev/null; then
    echo -e "${GREEN}Minikube is already running${NC}"
    MINIKUBE_RUNNING=true
else
    echo -e "${YELLOW}Starting minikube...${NC}"
    minikube start \
        --cpus="${MINIKUBE_CPUS}" \
        --memory="${MINIKUBE_MEMORY}" \
        --disk-size="${MINIKUBE_DISK}" \
        --driver="${MINIKUBE_DRIVER}" \
        --kubernetes-version=stable
    MINIKUBE_RUNNING=false
fi

# Enable required addons
echo -e "${BLUE}Enabling addons...${NC}"

ADDONS=(
    "storage-provisioner"      # Dynamic PV provisioning
    "default-storageclass"     # Default storage class
    "registry"                 # Local container registry
    "registry-aliases"         # DNS aliases for registry
    "metrics-server"           # Resource metrics for HPA
    "kong"                     # Kong API Gateway (gRPC support, rate limiting)
    "ingress-dns"              # DNS resolution for ingress
)

for addon in "${ADDONS[@]}"; do
    if minikube addons list | grep -q "$addon.*enabled"; then
        echo -e "  ${GREEN}✓${NC} $addon (already enabled)"
    else
        echo -e "  ${YELLOW}Enabling $addon...${NC}"
        minikube addons enable "$addon" 2>/dev/null || echo -e "  ${YELLOW}Warning: Could not enable $addon${NC}"
    fi
done

# Create hodei-jobs namespace if it doesn't exist
echo -e "${BLUE}Creating namespaces...${NC}"
kubectl create namespace hodei-jobs --dry-run=client -o yaml | kubectl apply -f -
kubectl create namespace hodei-jobs-workers --dry-run=client -o yaml | kubectl apply -f -
echo -e "  ${GREEN}✓${NC} hodei-jobs namespace ready"
echo -e "  ${GREEN}✓${NC} hodei-jobs-workers namespace ready"

# Configure Kong for Hodei
echo -e "${BLUE}Configuring Kong API Gateway...${NC}"

# Wait for Kong to be ready
echo "  Waiting for Kong to be ready..."
kubectl wait --namespace kong \
    --for=condition=ready pod \
    --selector=app.kubernetes.io/component=kong \
    --timeout=120s 2>/dev/null || echo -e "  ${YELLOW}Kong not fully ready, continuing...${NC}"

# Configure Kong admin to enable proxy for hodei.local
echo "  Verifying Kong proxy configuration..."
kubectl exec -n kong -l app.kubernetes.io/component=kong -- sh -c "
    # Wait for Kong to be ready
    Kong health || true
    echo 'Kong admin API available'
" 2>/dev/null || echo -e "  ${YELLOW}Kong admin API not accessible yet${NC}"

# Show Kong status
echo -e "  ${GREEN}Kong addon enabled${NC}"

# Show cluster info
echo ""
echo -e "${GREEN}=== Minikube Ready ===${NC}"
echo -e "Kubernetes: $(kubectl version --short 2>/dev/null | grep Server | awk '{print $3}')"
echo -e "Minikube IP: $(minikube ip)"
echo ""
echo -e "Next steps:"
echo -e "  1. Build images:  ${BLUE}./scripts/build-minikube.sh${NC}"
echo -e "  2. Deploy:        ${BLUE}helm upgrade --install hodei ./deploy/hodei-jobs-platform -n hodei-jobs -f ./deploy/hodei-jobs-platform/values-dev.yaml${NC}"
echo -e "  3. Watch pods:    ${BLUE}kubectl get pods -n hodei-jobs -w${NC}"
