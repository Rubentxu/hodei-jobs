#!/bin/bash
# ==============================================================================
# Hodei Jobs Platform - Kubernetes Production Setup Script
# ==============================================================================
# This script sets up all necessary infrastructure for Kubernetes provider
# to work in production mode with just job-k8s-xxx commands.
#
# Usage: ./scripts/setup-k8s-production.sh [OPTIONS]
# Options:
#   --cluster-context    Kubernetes context to use (default: kind-hodei-test)
#   --namespace          Namespace for workers (default: hodei-jobs-workers)
#   --build-image        Build worker image locally
#   --registry-url       Registry URL (e.g., registry.company.com)
#   --dry-run            Show what would be done without executing
# ==============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
CLUSTER_CONTEXT="${CLUSTER_CONTEXT:-kind-hodei-test}"
NAMESPACE="${NAMESPACE:-hodei-jobs-workers}"
BUILD_IMAGE=false
REGISTRY_URL=""
DRY_RUN=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --cluster-context)
            CLUSTER_CONTEXT="$2"
            shift 2
            ;;
        --namespace)
            NAMESPACE="$2"
            shift 2
            ;;
        --build-image)
            BUILD_IMAGE=true
            shift
            ;;
        --registry-url)
            REGISTRY_URL="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --cluster-context    Kubernetes context (default: kind-hodei-test)"
            echo "  --namespace          Namespace (default: hodei-jobs-workers)"
            echo "  --build-image        Build worker image"
            echo "  --registry-url       Registry URL for pushing image"
            echo "  --dry-run            Show actions without executing"
            echo "  -h, --help           Show this help"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     Hodei Jobs Platform - Kubernetes Production Setup        ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Configuration:"
echo "  Cluster Context:  ${CLUSTER_CONTEXT}"
echo "  Namespace:        ${NAMESPACE}"
echo "  Build Image:      ${BUILD_IMAGE}"
echo "  Registry URL:     ${REGISTRY_URL:-<none>}"
echo "  Dry Run:          ${DRY_RUN}"
echo ""

# Function to execute or show command
run_cmd() {
    if [[ "${DRY_RUN}" == "true" ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would execute: $*"
    else
        echo -e "${BLUE}[RUN]${NC} $*"
        "$@"
    fi
}

# Function to check prerequisites
check_prerequisites() {
    echo -e "${BLUE}📋 Checking prerequisites...${NC}"

    # Check kubectl
    if ! command -v kubectl &> /dev/null; then
        echo -e "${RED}❌ kubectl is not installed${NC}"
        exit 1
    fi
    echo -e "  ${GREEN}✓${NC} kubectl is installed"

    # Check cluster connectivity
    if ! kubectl cluster-info --context="${CLUSTER_CONTEXT}" &> /dev/null; then
        echo -e "${RED}❌ Cannot connect to cluster '${CLUSTER_CONTEXT}'${NC}"
        exit 1
    fi
    echo -e "  ${GREEN}✓${NC} Cluster '${CLUSTER_CONTEXT}' is accessible"

    # Check docker (for building image)
    if [[ "${BUILD_IMAGE}" == "true" ]] || [[ -n "${REGISTRY_URL}" ]]; then
        if ! command -v docker &> /dev/null; then
            echo -e "${RED}❌ docker is not installed (required for building image)${NC}"
            exit 1
        fi
        echo -e "  ${GREEN}✓${NC} docker is installed"
    fi

    echo -e "${GREEN}✅ Prerequisites check passed${NC}\n"
}

# Function to build worker image
build_worker_image() {
    echo -e "${BLUE}🔨 Building worker image...${NC}"

    if [[ "${BUILD_IMAGE}" == "false" ]]; then
        echo -e "${YELLOW}⚠ Skipping image build (use --build-image to enable)${NC}\n"
        return
    fi

    # Build binary first
    echo "  Building hodei-worker binary..."
    run_cmd cargo build --release -p hodei-worker-bin

    # Build image
    echo "  Building Docker image..."
    run_cmd docker build -f Dockerfile.worker -t hodei-jobs-worker:latest .

    # Tag for registry if provided
    if [[ -n "${REGISTRY_URL}" ]]; then
        echo "  Tagging image for registry..."
        run_cmd docker tag hodei-jobs-worker:latest "${REGISTRY_URL}/hodei-jobs-worker:latest"

        echo -e "${YELLOW}  To push to registry, run:${NC}"
        echo "    docker push ${REGISTRY_URL}/hodei-jobs-worker:latest"
    fi

    echo -e "${GREEN}✅ Worker image built${NC}\n"
}

# Function to create namespace
create_namespace() {
    echo -e "${BLUE}📦 Setting up namespace '${NAMESPACE}'...${NC}"

    # Check if namespace exists
    if kubectl get namespace "${NAMESPACE}" --context="${CLUSTER_CONTEXT}" &> /dev/null; then
        echo -e "  ${YELLOW}⚠${NC} Namespace '${NAMESPACE}' already exists"
    else
        run_cmd kubectl create namespace "${NAMESPACE}" --context="${CLUSTER_CONTEXT}"
        echo -e "  ${GREEN}✓${NC} Namespace created"
    fi

    # Apply Pod Security Standards
    echo "  Applying Pod Security Standards..."
    run_cmd kubectl label namespace "${NAMESPACE}" \
        pod-security.kubernetes.io/enforce=restricted \
        pod-security.kubernetes.io/audit=restricted \
        pod-security.kubernetes.io/warn=restricted \
        --context="${CLUSTER_CONTEXT}" --overwrite

    echo -e "${GREEN}✅ Namespace configured${NC}\n"
}

# Function to setup RBAC
setup_rbac() {
    echo -e "${BLUE}🔐 Setting up RBAC...${NC}"

    # Create ServiceAccount
    echo "  Creating ServiceAccount..."
    run_cmd kubectl create serviceaccount hodei-jobs-worker \
        --namespace="${NAMESPACE}" \
        --context="${CLUSTER_CONTEXT}"

    # Create Role
    echo "  Creating Role..."
    run_cmd kubectl create role hodei-jobs-worker-role \
        --namespace="${NAMESPACE}" \
        --verb=get,list,watch,create,delete,deletecollection \
        --resource=pods,pods/log,pods/status \
        --context="${CLUSTER_CONTEXT}"

    # Create RoleBinding
    echo "  Creating RoleBinding..."
    run_cmd kubectl create rolebinding hodei-jobs-worker-binding \
        --namespace="${NAMESPACE}" \
        --role=hodei-jobs-worker-role \
        --serviceaccount="${NAMESPACE}:hodei-jobs-worker" \
        --context="${CLUSTER_CONTEXT}"

    echo -e "${GREEN}✅ RBAC configured${NC}\n"
}

# Function to setup resource quotas
setup_quotas() {
    echo -e "${BLUE}📊 Setting up resource quotas...${NC}"

    # Create ResourceQuota
    run_cmd kubectl apply -f - --context="${CLUSTER_CONTEXT}" <<EOF
apiVersion: v1
kind: ResourceQuota
metadata:
  name: hodei-jobs-workers-quota
  namespace: ${NAMESPACE}
spec:
  hard:
    pods: "100"
    requests.cpu: "50"
    requests.memory: 200Gi
    limits.cpu: "100"
    limits.memory: 400Gi
EOF

    # Create LimitRange
    run_cmd kubectl apply -f - --context="${CLUSTER_CONTEXT}" <<EOF
apiVersion: v1
kind: LimitRange
metadata:
  name: hodei-jobs-workers-limits
  namespace: ${NAMESPACE}
spec:
  limits:
  - default:
      cpu: "2"
      memory: "4Gi"
    defaultRequest:
      cpu: "500m"
      memory: "1Gi"
    type: Container
EOF

    echo -e "${GREEN}✅ Resource quotas configured${NC}\n"
}

# Function to create image pull secret
create_image_pull_secret() {
    if [[ -z "${REGISTRY_URL}" ]]; then
        return
    fi

    echo -e "${BLUE}🔑 Setting up image pull secret...${NC}"

    echo -e "${YELLOW}  Registry: ${REGISTRY_URL}${NC}"
    echo -e "${YELLOW}  Please provide registry credentials:${NC}"

    read -p "  Username: " REGISTRY_USERNAME
    read -s -p "  Password: " REGISTRY_PASSWORD
    echo ""

    run_cmd kubectl create secret docker-registry regcred \
        --docker-server="${REGISTRY_URL}" \
        --docker-username="${REGISTRY_USERNAME}" \
        --docker-password="${REGISTRY_PASSWORD}" \
        --namespace="${NAMESPACE}" \
        --context="${CLUSTER_CONTEXT}"

    echo -e "${GREEN}✅ Image pull secret created${NC}\n"
}

# Function to verify setup
verify_setup() {
    echo -e "${BLUE}🔍 Verifying setup...${NC}"

    # Check namespace
    if kubectl get namespace "${NAMESPACE}" --context="${CLUSTER_CONTEXT}" &> /dev/null; then
        echo -e "  ${GREEN}✓${NC} Namespace '${NAMESPACE}' exists"
    else
        echo -e "  ${RED}✗${NC} Namespace '${NAMESPACE}' not found"
        return 1
    fi

    # Check RBAC
    if kubectl get serviceaccount hodei-jobs-worker --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" &> /dev/null; then
        echo -e "  ${GREEN}✓${NC} ServiceAccount exists"
    else
        echo -e "  ${RED}✗${NC} ServiceAccount not found"
        return 1
    fi

    if kubectl get role hodei-jobs-worker-role --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" &> /dev/null; then
        echo -e "  ${GREEN}✓${NC} Role exists"
    else
        echo -e "  ${RED}✗${NC} Role not found"
        return 1
    fi

    if kubectl get rolebinding hodei-jobs-worker-binding --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" &> /dev/null; then
        echo -e "  ${GREEN}✓${NC} RoleBinding exists"
    else
        echo -e "  ${RED}✗${NC} RoleBinding not found"
        return 1
    fi

    # Check quotas
    if kubectl get resourcequota hodei-jobs-workers-quota --namespace="${NAMESPACE}" --context="${CLUSTER_CONTEXT}" &> /dev/null; then
        echo -e "  ${GREEN}✓${NC} ResourceQuota exists"
    else
        echo -e "  ${RED}✗${NC} ResourceQuota not found"
        return 1
    fi

    echo -e "${GREEN}✅ All checks passed${NC}\n"
}

# Function to show next steps
show_next_steps() {
    echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║                    Setup Complete! 🎉                         ║${NC}"
    echo -e "${GREEN}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "Next steps:"
    echo ""
    echo -e "${BLUE}1. Build and start the server:${NC}"
    echo "   export HODEI_KUBERNETES_ENABLED=1"
    echo "   export HODEI_K8S_NAMESPACE=${NAMESPACE}"
    echo "   export HODEI_K8S_CONTEXT=${CLUSTER_CONTEXT}"
    if [[ -n "${REGISTRY_URL}" ]]; then
        echo "   export HODEI_WORKER_IMAGE=${REGISTRY_URL}/hodei-jobs-worker:latest"
    fi
    echo "   cargo run --package hodei-server-bin --release"
    echo ""
    echo -e "${BLUE}2. Test Kubernetes provider:${NC}"
    echo "   just job-k8s-hello"
    echo ""
    echo -e "${BLUE}3. Monitor pods:${NC}"
    echo "   kubectl get pods --namespace=${NAMESPACE} --watch"
    echo ""
    echo -e "${BLUE}4. Check provider status:${NC}"
    echo "   kubectl get svc,role,rolebinding,serviceaccount --namespace=${NAMESPACE}"
    echo ""
}

# Main execution
main() {
    check_prerequisites
    build_worker_image
    create_namespace
    setup_rbac
    setup_quotas
    create_image_pull_secret
    verify_setup
    show_next_steps
}

# Run main function
main

echo -e "${GREEN}✅ Production setup completed successfully!${NC}"
