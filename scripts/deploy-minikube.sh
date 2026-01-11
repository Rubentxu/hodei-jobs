#!/bin/bash
# Hodei Jobs Platform Deployment Script for Minikube
# Usage: ./scripts/deploy-minikube.sh [command]
#
# Commands:
#   up        - Start Minikube and deploy
#   down      - Stop Minikube
#   rebuild   - Rebuild images and redeploy
#   logs      - Show operator logs
#   dashboard - Open Kubernetes dashboard
#   uninstall - Remove all resources

set -e

CLUSTER_NAME="${CLUSTER_NAME:-hodei-cluster}"
NAMESPACE="${NAMESPACE:-hodei-system}"
OPERATOR_IMAGE="hodei/operator:${VERSION:-v0.38.5}"
WEB_IMAGE="hodei/operator-web:${VERSION:-v0.38.5}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    local missing=()

    for cmd in minikube kubectl docker helm; do
        if ! command -v $cmd &> /dev/null; then
            missing+=($cmd)
        fi
    done

    if [ ${#missing[@]} -ne 0 ]; then
        log_error "Missing required tools: ${missing[*]}"
        echo "Install with:"
        echo "  - minikube: https://minikube.sigs.k8s.io/docs/start/"
        echo "  - kubectl: https://kubernetes.io/docs/tasks/tools/"
        echo "  - docker: https://docs.docker.com/get-docker/"
        echo "  - helm: https://helm.sh/docs/intro/install/"
        exit 1
    fi

    log_info "All prerequisites met"
}

# Start Minikube cluster
start_minikube() {
    log_info "Starting Minikube cluster: $CLUSTER_NAME"

    if minikube status -p $CLUSTER_NAME &> /dev/null; then
        log_info "Cluster already running"
    else
        minikube start -p $CLUSTER_NAME \
            --driver=docker \
            --cpus=4 \
            --memory=8192 \
            --kubernetes-version=v1.28.0 \
            --registry-mirror=https://registry.docker-cn.com
    fi

    # Enable ingress addon
    log_info "Enabling Ingress addon..."
    minikube addons enable ingress -p $CLUSTER_NAME

    # Enable metrics-server
    log_info "Enabling Metrics Server..."
    minikube addons enable metrics-server -p $CLUSTER_NAME

    log_info "Cluster started successfully"
    minikube ip -p $CLUSTER_NAME
}

# Build Docker images
build_images() {
    log_info "Building Docker images..."

    # Set Minikube Docker daemon
    eval $(minikube -p $CLUSTER_NAME docker-env)

    # Build operator image
    log_info "Building operator image: $OPERATOR_IMAGE"
    docker build -t $OPERATOR_IMAGE -f crates/operator/Dockerfile .

    # Build web dashboard image
    log_info "Building web dashboard image: $WEB_IMAGE"
    docker build -t $WEB_IMAGE -f crates/operator-web/Dockerfile .

    log_info "Images built successfully"
}

# Deploy with Helm
deploy_helm() {
    log_info "Deploying Hodei Operator with Helm..."

    # Create namespace if not exists
    kubectl create namespace $NAMESPACE --dry-run=client -o yaml | kubectl apply -f -

    # Install/upgrade operator
    helm upgrade --install hodei-operator ./helm/operator \
        --namespace $NAMESPACE \
        --set image.repository=${OPERATOR_IMAGE%:*} \
        --set image.tag=${OPERATOR_IMAGE#*:} \
        --set web.image.repository=${WEB_IMAGE%:*} \
        --set web.image.tag=${WEB_IMAGE#*:} \
        --set hodeiServer.address="http://hodei-server:50051" \
        --set web.ingress.enabled=true \
        --set web.ingress.hosts[0].host=hodei.local \
        --wait \
        --timeout 300s

    log_info "Operator deployed successfully"
}

# Deploy Hodei Server (required dependency)
deploy_server() {
    log_info "Deploying Hodei Server..."

    # Deploy PostgreSQL
    log_info "Deploying PostgreSQL..."
    helm upgrade --install hodei-postgres ./helm/hodei-jobs-platform/charts/postgres \
        --namespace $NAMESPACE \
        --wait \
        --timeout 120s 2>/dev/null || log_warn "PostgreSQL chart not found, skipping"

    # Deploy NATS
    log_info "Deploying NATS..."
    helm upgrade --install hodei-nats ./helm/hodei-jobs-platform/charts/nats \
        --namespace $NAMESPACE \
        --wait \
        --timeout 120s 2>/dev/null || log_warn "NATS chart not found, skipping"

    # Deploy Server
    log_info "Deploying Hodei Server..."
    helm upgrade --install hodei-server ./helm/hodei-jobs-platform/charts/hodei-server \
        --namespace $NAMESPACE \
        --set image.repository=hodei/server \
        --set image.tag=${VERSION:-latest} \
        --wait \
        --timeout 120s 2>/dev/null || log_warn "Server chart not found, skipping"
}

# Wait for deployments
wait_for_deployments() {
    log_info "Waiting for deployments to be ready..."

    kubectl wait --namespace $NAMESPACE \
        --for=condition=available \
        --timeout=300s \
        deployment/hodei-operator 2>/dev/null || log_warn "Operator deployment not ready yet"

    log_info "Deployments ready"
}

# Show status
show_status() {
    log_info "Cluster status:"
    kubectl cluster-info
    echo ""

    log_info "Nodes:"
    kubectl get nodes
    echo ""

    log_info "Pods in $NAMESPACE:"
    kubectl get pods -n $NAMESPACE
    echo ""

    log_info "Services:"
    kubectl get svc -n $NAMESPACE
    echo ""

    log_info "Ingress:"
    kubectl get ingress -n $NAMESPACE
}

# Show logs
show_logs() {
    log_info "Following operator logs..."
    kubectl logs -n $NAMESPACE -l app.kubernetes.io/name=hodei-operator -f
}

# Open dashboard
open_dashboard() {
    log_info "Opening Kubernetes dashboard..."
    minikube dashboard -p $CLUSTER_NAME
}

# Stop Minikube
stop_minikube() {
    log_info "Stopping Minikube cluster..."
    minikube stop -p $CLUSTER_NAME
}

# Uninstall everything
uninstall() {
    log_info "Uninstalling Hodei Operator..."
    helm uninstall hodei-operator -n $NAMESPACE 2>/dev/null || true

    log_info "Uninstalling Hodei Server..."
    helm uninstall hodei-server -n $NAMESPACE 2>/dev/null || true
    helm uninstall hodei-postgres -n $NAMESPACE 2>/dev/null || true
    helm uninstall hodei-nats -n $NAMESPACE 2>/dev/null || true

    log_info "Deleting namespace..."
    kubectl delete namespace $NAMESPACE 2>/dev/null || true

    log_info "Uninstallation complete"
}

# Main command handler
case "${1:-up}" in
    up)
        check_prerequisites
        start_minikube
        build_images
        deploy_server
        deploy_helm
        wait_for_deployments
        show_status

        echo ""
        log_info "========================================="
        log_info "Hodei Platform deployed successfully!"
        log_info "========================================="
        echo ""
        echo "Dashboard URL: http://hodei.local"
        echo "Add to /etc/hosts:"
        echo "  $(minikube ip -p $CLUSTER_NAME) hodei.local"
        echo ""
        ;;
    down)
        stop_minikube
        ;;
    rebuild)
        build_images
        kubectl rollout restart deployment/hodei-operator -n $NAMESPACE
        wait_for_deployments
        ;;
    logs)
        show_logs
        ;;
    dashboard)
        open_dashboard
        ;;
    uninstall)
        read -p "Are you sure you want to uninstall? (y/n): " confirm
        if [ "$confirm" = "y" ]; then
            uninstall
        fi
        ;;
    *)
        echo "Usage: $0 [command]"
        echo ""
        echo "Commands:"
        echo "  up        - Start Minikube and deploy (default)"
        echo "  down      - Stop Minikube"
        echo "  rebuild   - Rebuild images and redeploy"
        echo "  logs      - Show operator logs"
        echo "  dashboard - Open Kubernetes dashboard"
        echo "  uninstall - Remove all resources"
        exit 1
        ;;
esac
