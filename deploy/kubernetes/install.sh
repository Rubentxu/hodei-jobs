#!/bin/bash
# Hodei Kubernetes Provider Installation Script
# This script sets up the necessary Kubernetes resources for the Hodei Kubernetes Provider

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== Hodei Kubernetes Provider Installation ==="
echo ""

# Check if kubectl is available
if ! command -v kubectl &> /dev/null; then
    echo "Error: kubectl is not installed or not in PATH"
    exit 1
fi

# Check cluster connectivity
echo "Checking cluster connectivity..."
if ! kubectl cluster-info &> /dev/null; then
    echo "Error: Cannot connect to Kubernetes cluster"
    echo "Please ensure your kubeconfig is properly configured"
    exit 1
fi

echo "Connected to cluster: $(kubectl config current-context)"
echo ""

# Create hodei-system namespace if it doesn't exist
echo "Creating hodei-system namespace..."
kubectl create namespace hodei-system --dry-run=client -o yaml | kubectl apply -f -

# Apply manifests in order
echo "Applying namespace..."
kubectl apply -f "${SCRIPT_DIR}/namespace.yaml"

echo "Applying RBAC..."
kubectl apply -f "${SCRIPT_DIR}/rbac.yaml"

echo "Applying network policies..."
kubectl apply -f "${SCRIPT_DIR}/network-policy.yaml"

echo "Applying configmap..."
kubectl apply -f "${SCRIPT_DIR}/configmap.yaml"

echo ""
echo "=== Installation Complete ==="
echo ""
echo "Namespaces created:"
echo "  - hodei-system (control plane)"
echo "  - hodei-workers (worker pods)"
echo ""
echo "To verify the installation:"
echo "  kubectl get all -n hodei-workers"
echo "  kubectl get networkpolicies -n hodei-workers"
echo ""
echo "To enable the Kubernetes provider in Hodei, set:"
echo "  HODEI_K8S_ENABLED=1"
echo "  HODEI_K8S_NAMESPACE=hodei-workers"
echo ""
echo "For in-cluster deployment, the control plane ServiceAccount"
echo "'hodei-control-plane' in 'hodei-system' namespace has the"
echo "necessary permissions to manage worker pods."
