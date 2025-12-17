#!/bin/bash
# =============================================================================
# Hodei Job Platform - Rapid Development Script
# =============================================================================
# This script provides a one-command development workflow with hot reload
# for both backend (Rust) and frontend (React) with database.
#
# Usage:
#   ./scripts/dev.sh              # Start full development environment
#   ./scripts/dev.sh db           # Start only database
#   ./scripts/dev.sh backend      # Start only backend
#   ./scripts/dev.sh frontend     # Start only frontend
#   ./scripts/dev.sh test         # Run tests
#   ./scripts/dev.sh clean        # Clean everything
# =============================================================================

set -e

# Determine project root and change to it
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Function to print colored output
print_header() {
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}$1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${CYAN}ℹ️  $1${NC}"
}

# Check if required tools are installed
check_requirements() {
    print_header "Checking Requirements"

    local missing_tools=()

    # Check cargo
    if ! command -v cargo &> /dev/null; then
        missing_tools+=("cargo (Rust)")
    fi

    # Check node
    if ! command -v node &> /dev/null; then
        missing_tools+=("node")
    fi

    # Check npm
    if ! command -v npm &> /dev/null; then
        missing_tools+=("npm")
    fi

    # Check docker
    if ! command -v docker &> /dev/null; then
        missing_tools+=("docker")
    fi

    # Check docker-compose
    if ! command -v docker-compose &> /dev/null; then
        missing_tools+=("docker-compose")
    fi

    # Check just
    if ! command -v just &> /dev/null; then
        print_warning "just not found. Installing..."
        cargo install --locked just
    fi

    # Check bacon
    if ! command -v bacon &> /dev/null; then
        print_warning "bacon not found. Installing..."
        cargo install --locked bacon
    fi

    if [ ${#missing_tools[@]} -ne 0 ]; then
        print_error "Missing required tools:"
        for tool in "${missing_tools[@]}"; do
            echo -e "  ${RED}- $tool${NC}"
        done
        echo ""
        print_info "Please install the missing tools and try again."
        exit 1
    fi

    print_success "All requirements satisfied"
}

# Start database
start_database() {
    print_header "Starting Database"
    docker-compose -f docker-compose.dev.yml up -d postgres

    # Wait for database to be ready
    print_info "Waiting for database to be ready..."
    timeout=30
    while ! docker exec hodei-jobs-postgres pg_isready -U postgres -d hodei &> /dev/null; do
        if [ $timeout -eq 0 ]; then
            print_error "Database failed to start"
            exit 1
        fi
        sleep 1
        timeout=$((timeout-1))
    done

    print_success "Database is ready at localhost:5432"
    print_info "User: postgres, Password: postgres, Database: hodei"
}

# Start backend with hot reload
start_backend() {
    print_header "Starting Backend (Rust) with Hot Reload"

    # Ensure dependencies are installed
    if [ ! -d "target" ]; then
        print_info "Building dependencies for the first time..."
        cargo check --workspace
    fi

    # Start with bacon
    print_success "Starting backend with bacon (hot reload enabled)"
    print_info "Commands: Ctrl+C to stop"
    print_info "Bacon commands: :q to quit, :j job-name to switch job\n"

    cd crates/grpc && bacon run
}

# Start frontend with hot reload
start_frontend() {
    print_header "Starting Frontend (React) with HMR"

    # Ensure dependencies are installed
    if [ ! -d "web/node_modules" ]; then
        print_info "Installing frontend dependencies..."
        cd web && npm install
        cd ..
    fi

    print_success "Starting frontend with Vite (HMR enabled)"
    print_info "Frontend running at http://localhost:5173"
    print_info "HMR: Changes will automatically reflect in browser\n"

    cd web && npm run dev
}

# Run tests
run_tests() {
    print_header "Running Tests"

    print_info "Running backend tests..."
    cargo test --workspace

    print_info "\nRunning frontend tests..."
    cd web && npm test -- --run

    print_success "All tests completed"
}

# Clean everything
clean_all() {
    print_header "Cleaning Everything"

    print_info "Cleaning Rust build artifacts..."
    cargo clean

    print_info "Cleaning frontend dependencies..."
    cd web && rm -rf node_modules dist && cd ..

    print_info "Removing Docker volumes..."
    docker-compose -f docker-compose.dev.yml down -v

    print_success "Clean complete"
}

# Show help
show_help() {
    echo "Hodei Job Platform - Development Script"
    echo ""
    echo "Usage: ./scripts/dev.sh [command]"
    echo ""
    echo "Commands:"
    echo "  (no command)  Start full development environment"
    echo "  db            Start only database"
    echo "  backend       Start only backend with hot reload"
    echo "  frontend      Start only frontend with HMR"
    echo "  test          Run all tests"
    echo "  clean         Clean everything"
    echo "  help          Show this help message"
    echo ""
    echo "Environment Variables:"
    echo "  HODEI_DEV_MODE=1    Enable development mode"
    echo "  RUST_LOG=debug      Set Rust log level"
    echo ""
    echo "Quick Start:"
    echo "  ./scripts/dev.sh    Start everything"
    echo "  just dev-db         Alternative: just command"
    echo "  just dev            Alternative: just command"
    echo ""
}

# Main execution
main() {
    case "${1:-}" in
        "db")
            check_requirements
            start_database
            ;;
        "backend")
            check_requirements
            start_backend
            ;;
        "frontend")
            check_requirements
            start_frontend
            ;;
        "test")
            check_requirements
            run_tests
            ;;
        "clean")
            clean_all
            ;;
        "help"|"-h"|"--help")
            show_help
            ;;
        "")
            # Start full environment
            check_requirements
            start_database

            print_header "Starting Development Environment"
            print_success "Database: Running"
            print_info "Starting backend and frontend..."
            print_info "Press Ctrl+C to stop all services\n"

            # Start backend and frontend in parallel
            (start_backend) &
            BACKEND_PID=$!

            sleep 3

            (start_frontend) &
            FRONTEND_PID=$!

            # Wait for user interrupt
            trap "kill $BACKEND_PID $FRONTEND_PID 2>/dev/null; exit" INT
            wait
            ;;
        *)
            print_error "Unknown command: $1"
            show_help
            exit 1
            ;;
    esac
}

# Run main function
main "$@"
