#!/bin/bash
# =============================================================================
# Multi-Provider Integration Test Script
# =============================================================================
# Tests both Docker and Kubernetes providers working together
# =============================================================================

set -e

echo "🚀 Multi-Provider Integration Tests"
echo "===================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}ℹ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Check if Docker is available
check_docker() {
    if ! command -v docker &> /dev/null; then
        print_error "Docker is not installed or not in PATH"
        exit 1
    fi

    if ! docker info &> /dev/null; then
        print_error "Docker daemon is not running"
        print_info "Try: sudo systemctl start docker"
        exit 1
    fi

    print_success "Docker is available"
}

# Run Docker provider tests
run_docker_tests() {
    echo ""
    echo "🐳 Testing Docker Provider"
    echo "--------------------------"
    cargo test -p hodei-server-infrastructure --test multi_provider_integration test_docker_provider_basic_operations -- --nocapture
    print_success "Docker provider tests passed"
}

# Run provider selection tests
run_selection_tests() {
    echo ""
    echo "🎯 Testing Provider Selection Strategies"
    echo "----------------------------------------"
    cargo test -p hodei-server-infrastructure --test multi_provider_integration test_provider_selection_by_labels -- --nocapture
    print_success "Provider selection tests passed"
}

# Run concurrent workers tests
run_concurrent_tests() {
    echo ""
    echo "🔀 Testing Concurrent Workers"
    echo "-----------------------------"
    cargo test -p hodei-server-infrastructure --test multi_provider_integration test_concurrent_workers_on_both_providers -- --nocapture
    print_success "Concurrent workers tests passed"
}

# Run log streaming tests
run_log_tests() {
    echo ""
    echo "📜 Testing Log Streaming"
    echo "------------------------"
    cargo test -p hodei-server-infrastructure --test multi_provider_integration test_log_streaming_from_both_providers -- --nocapture
    print_success "Log streaming tests passed"
}

# Run Kubernetes tests (if enabled)
run_kubernetes_tests() {
    if [ "$HODEI_K8S_TEST" = "1" ]; then
        echo ""
        echo "☸️  Testing Kubernetes Provider"
        echo "-------------------------------"
        cargo test -p hodei-server-infrastructure --test multi_provider_integration test_kubernetes_provider_basic_operations -- --nocapture
        print_success "Kubernetes provider tests passed"

        echo ""
        echo "🚀 Testing GPU Workers on Kubernetes"
        echo "-------------------------------------"
        cargo test -p hodei-server-infrastructure --test multi_provider_integration test_gpu_worker_on_kubernetes -- --nocapture
        print_success "GPU worker tests passed"
    else
        print_info "Kubernetes tests skipped (set HODEI_K8S_TEST=1 to enable)"
    fi
}

# Run all tests
run_all_tests() {
    echo ""
    echo "🧪 Running All Multi-Provider Tests"
    echo "------------------------------------"
    cargo test -p hodei-server-infrastructure --test multi_provider_integration -- --nocapture
    print_success "All multi-provider tests passed"
}

# Main execution
main() {
    # Check Docker
    check_docker

    # Parse arguments
    case "${1:-all}" in
        docker)
            run_docker_tests
            run_selection_tests
            run_concurrent_tests
            run_log_tests
            ;;
        kubernetes)
            run_kubernetes_tests
            ;;
        all)
            run_all_tests
            ;;
        help|--help|-h)
            echo "Usage: $0 [docker|kubernetes|all|help]"
            echo ""
            echo "Commands:"
            echo "  docker      Run Docker provider tests (default)"
            echo "  kubernetes  Run Kubernetes provider tests (requires HODEI_K8S_TEST=1)"
            echo "  all         Run all multi-provider tests"
            echo "  help        Show this help message"
            echo ""
            echo "Environment Variables:"
            echo "  HODEI_K8S_TEST=1   Enable Kubernetes tests"
            echo ""
            echo "Examples:"
            echo "  $0 docker              # Run Docker tests"
            echo "  HODEI_K8S_TEST=1 $0 all    # Run all tests including Kubernetes"
            exit 0
            ;;
        *)
            print_error "Unknown command: $1"
            echo "Use '$0 help' for usage information"
            exit 1
            ;;
    esac

    echo ""
    echo "===================================="
    print_success "All tests completed successfully!"
    echo "===================================="
}

# Run main function
main "$@"
