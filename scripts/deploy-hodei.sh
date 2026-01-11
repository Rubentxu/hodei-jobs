#!/bin/bash

# =============================================================================
# Hodei Jobs Platform - Script de Despliegue Automático
# =============================================================================
# Este script automatiza el despliegue completo del sistema Hodei Jobs Platform
# en un cluster Minikube local.
#
# Uso: ./scripts/deploy-hodei.sh [opciones]
#   --setup-minikube    : Crear cluster Minikube
#   --install-infra     : Instalar PostgreSQL + NATS
#   --install-operator  : Instalar el operator
#   --install-all       : Ejecutar todo (por defecto)
#   --verify            : Verificar instalación
#   --destroy           : Destruir todo
# =============================================================================

set -euo pipefail

# Colores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Variables de configuración
NAMESPACE="hodei-system"
OPERATOR_VERSION="${OPERATOR_VERSION:-latest}"
MINIKUBE_CPUS="${MINIKUBE_CPUS:-4}"
MINIKUBE_MEMORY="${MINIKUBE_MEMORY:-8g}"
HELM_CHART_PATH="./helm/hodei-jobs-platform"
OPERATOR_CHART_PATH="./helm/operator"

# Funciones de logging
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Verificar prerrequisitos
check_prerequisites() {
    log_info "Verificando prerrequisitos..."

    local missing=()

    for cmd in kubectl helm docker minikube; do
        if ! command -v $cmd &> /dev/null; then
            missing+=($cmd)
            log_error "$cmd no está instalado"
        fi
    done

    if [ ${#missing[@]} -ne 0 ]; then
        log_error "Faltan prerrequisitos: ${missing[*]}"
        echo "Instala los prerrequisitos antes de continuar."
        exit 1
    fi

    log_success "Todos los prerrequisitos están instalados"
}

# Configurar Minikube
setup_minikube() {
    log_info "Configurando Minikube..."

    if minikube status &> /dev/null; then
        log_warn "Minikube ya está corriendo. Deteniendo..."
        minikube stop
    fi

    log_info "Creando cluster Minikube con ${MINIKUBE_CPUS} CPUs y ${MINIKUBE_MEMORY} RAM..."

    minikube start \
        --cpus=$MINIKUBE_CPUS \
        --memory=$MINIKUBE_MEMORY \
        --driver=docker \
        --kubernetes-version=v1.28.0 \
        --addons=registry \
        --addons=metrics-server

    # Configurar Docker para usar el daemon de Minikube
    eval $(minikube docker-env)

    log_success "Minikube creado y configurado"
}

# Instalar infraestructura (PostgreSQL + NATS)
install_infrastructure() {
    log_info "Instalando infraestructura (PostgreSQL + NATS)..."

    # Crear namespace
    kubectl create namespace $NAMESPACE --dry-run=client -o yaml | kubectl apply -f -

    # Instalar PostgreSQL
    log_info "Instalando PostgreSQL..."
    helm install postgres $HELM_CHART_PATH \
        --namespace $NAMESPACE \
        --set postgresql.enabled=true \
        --set postgresql.postgresqlDatabase=hodei_jobs \
        --set postgresql.persistence.size=1Gi \
        --wait --timeout 300s || {
            log_warn "PostgreSQL ya existe o falló la instalación"
        }

    # Instalar NATS
    log_info "Instalando NATS..."
    helm install nats $HELM_CHART_PATH \
        --namespace $NAMESPACE \
        --set nats.enabled=true \
        --set nats.persistence.size=1Gi \
        --wait --timeout 300s || {
            log_warn "NATS ya existe o falló la instalación"
        }

    log_success "Infraestructura instalada"
}

# Instalar el Operator
install_operator() {
    log_info "Instalando Hodei Operator..."

    # Configurar Docker para Minikube (si no está configurado)
    if [ -z "${DOCKER_HOST:-}" ]; then
        eval $(minikube docker-env)
    fi

    # Buildar imagen del operator
    log_info "Buildando imagen del operator..."
    docker build -t hodei/operator:$OPERATOR_VERSION ./crates/operator

    # Instalar operator con Helm
    log_info "Instalando operator con Helm..."
    helm upgrade --install hodei-operator $OPERATOR_CHART_PATH \
        --namespace $NAMESPACE \
        --create-namespace \
        --set image.repository=hodei/operator \
        --set image.tag=$OPERATOR_VERSION \
        --set hodeiServer.address="http://hodei-server:50051" \
        --set crds.create=true \
        --wait --timeout 300s

    log_success "Operator instalado"
}

# Instalar Server (opcional)
install_server() {
    log_info "Instalando Hodei Server..."

    # Configurar Docker para Minikube
    if [ -z "${DOCKER_HOST:-}" ]; then
        eval $(minikube docker-env)
    fi

    # Buildar imagen del server
    log_info "Buildando imagen del server..."
    docker build -t hodei/server:$OPERATOR_VERSION .

    # Instalar server con Helm
    log_info "Instalando server con Helm..."
    helm upgrade --install hodei-server $HELM_CHART_PATH \
        --namespace $NAMESPACE \
        --set server.enabled=true \
        --set server.image.repository=hodei/server \
        --set server.image.tag=$OPERATOR_VERSION \
        --set server.replicas=1 \
        --wait --timeout 300s

    log_success "Server instalado"
}

# Verificar instalación
verify_installation() {
    log_info "Verificando instalación..."

    echo ""
    echo "=============================================="
    echo "       ESTADO DEL CLUSTER HODEI             "
    echo "=============================================="
    echo ""

    # Verificar nodos
    echo -e "${BLUE}[NODOS]${NC}"
    kubectl get nodes || echo "No se pueden obtener los nodos"
    echo ""

    # Verificar pods
    echo -e "${BLUE}[PODS]${NC}"
    kubectl get pods -n $NAMESPACE || echo "No se pueden obtener los pods"
    echo ""

    # Verificar servicios
    echo -e "${BLUE}[SERVICIOS]${NC}"
    kubectl get svc -n $NAMESPACE || echo "No se pueden obtener los servicios"
    echo ""

    # Verificar CRDs
    echo -e "${BLUE}[CRDS]${NC}"
    kubectl get crds 2>/dev/null | grep hodei.io || echo "No se encontraron CRDs de Hodei"
    echo ""

    # Verificar operator
    echo -e "${BLUE}[OPERATOR]${NC}"
    kubectl get deployment hodei-operator -n $NAMESPACE 2>/dev/null || echo "Operator no encontrado"
    echo ""

    # Verificar jobs
    echo -e "${BLUE}[JOBS]${NC}"
    kubectl get jobs.hodei.io -n $NAMESPACE 2>/dev/null || echo "No hay jobs"
    echo ""

    echo "=============================================="
    echo "         COMANDOS ÚTILES                     "
    echo "=============================================="
    echo ""
    echo "Ver logs del operator:"
    echo "  kubectl logs -n $NAMESPACE -l app=hodei-operator -f"
    echo ""
    echo "Ver pods:"
    echo "  kubectl get pods -n $NAMESPACE -w"
    echo ""
    echo "Puerto forwarding al servidor gRPC:"
    echo "  kubectl port-forward -n $NAMESPACE svc/hodei-server 50051:50051"
    echo ""
    echo "Crear un job de prueba:"
    echo "  kubectl apply -f examples/crds/job-example.yaml -n $NAMESPACE"
    echo ""
}

# Destruir todo
destroy_all() {
    log_warn "Destruyendo instalación de Hodei..."

    # Desinstalar releases de Helm
    helm uninstall hodei-operator -n $NAMESPACE 2>/dev/null || true
    helm uninstall hodei-server -n $NAMESPACE 2>/dev/null || true
    helm uninstall postgres -n $NAMESPACE 2>/dev/null || true
    helm uninstall nats -n $NAMESPACE 2>/dev/null || true

    # Eliminar CRDs
    kubectl delete crd jobs.hodei.io 2>/dev/null || true
    kubectl delete crd workerpools.hodei.io 2>/dev/null || true
    kubectl delete crd providerconfigs.hodei.io 2>/dev/null || true

    # Eliminar namespace
    kubectl delete namespace $NAMESPACE 2>/dev/null || true

    # Opcional: eliminar cluster Minikube
    read -p "¿Eliminar también el cluster Minikube? (s/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Ss]$ ]]; then
        minikube delete
        log_success "Cluster Minikube eliminado"
    fi

    log_success "Instalación destruida"
}

# Mostrar ayuda
show_help() {
    echo "Hodei Jobs Platform - Script de Despliegue Automático"
    echo ""
    echo "Uso: $0 [opciones]"
    echo ""
    echo "Opciones:"
    echo "  --setup-minikube     Crear cluster Minikube"
    echo "  --install-infra      Instalar PostgreSQL + NATS"
    echo "  --install-operator   Instalar el operator"
    echo "  --install-server     Instalar el servidor"
    echo "  --install-all        Ejecutar todo (por defecto)"
    echo "  --verify             Verificar instalación"
    echo "  --destroy            Destruir todo"
    echo "  --help               Mostrar esta ayuda"
    echo ""
    echo "Variables de entorno:"
    echo "  OPERATOR_VERSION     Versión del operator (default: latest)"
    echo "  MINIKUBE_CPUS        CPUs para Minikube (default: 4)"
    echo "  MINIKUBE_MEMORY      RAM para Minikube (default: 8g)"
    echo ""
    echo "Ejemplos:"
    echo "  $0 --install-all                           # Desplegar todo"
    echo "  OPERATOR_VERSION=v0.38.5 $0 --install-all  # Con versión específica"
    echo "  MINIKUBE_CPUS=8 MINIKUBE_MEMORY=16g $0     # Con más recursos"
    echo "  $0 --verify                                # Verificar instalación"
    echo "  $0 --destroy                               # Destruir todo"
}

# Main
main() {
    local mode="${1:-install-all}"

    case $mode in
        --setup-minikube)
            check_prerequisites
            setup_minikube
            ;;
        --install-infra)
            install_infrastructure
            ;;
        --install-operator)
            install_operator
            ;;
        --install-server)
            install_server
            ;;
        --install-all)
            check_prerequisites
            setup_minikube
            install_infrastructure
            install_operator
            install_server
            verify_installation
            ;;
        --verify)
            verify_installation
            ;;
        --destroy)
            destroy_all
            ;;
        --help|-h)
            show_help
            ;;
        *)
            log_error "Opción desconocida: $mode"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
