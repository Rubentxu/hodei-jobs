#!/bin/bash
# =============================================================================
# Hodei Job Platform - Initial Setup Script
# =============================================================================
# This script sets up a complete development environment for new developers
# in less than 5 minutes with all necessary tools and dependencies.
#
# Usage:
#   ./setup.sh              # Full setup
#   ./setup.sh --minimal    # Minimal setup (skip optional tools)
#   ./setup.sh --help       # Show help
# =============================================================================

set -e

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

print_step() {
    echo -e "\n${YELLOW}▶ $1${NC}"
}

# Detect OS
detect_os() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        OS="linux"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        OS="macos"
    elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
        OS="windows"
    else
        OS="unknown"
    fi
}

# Check if running as root (Docker, etc.)
check_root() {
    if [ "$EUID" -eq 0 ]; then
        print_warning "Running as root. Some installations may fail."
        print_info "Consider running without root privileges when possible."
    fi
}

# Install Rust toolchain
install_rust() {
    print_step "Installing Rust toolchain..."

    if ! command -v cargo &> /dev/null; then
        print_info "Installing Rust (this may take a few minutes)..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
        print_success "Rust installed successfully"
    else
        print_success "Rust already installed: $(cargo --version)"
    fi

    # Install Rust development tools
    print_info "Installing Rust development tools..."
    rustup component add rustfmt clippy rust-src
    print_success "Rust tools installed"
}

# Install Node.js and npm
install_nodejs() {
    print_step "Installing Node.js and npm..."

    if ! command -v node &> /dev/null; then
        if command -v brew &> /dev/null; then
            print_info "Installing Node.js via Homebrew..."
            brew install node
        elif command -v apt-get &> /dev/null; then
            print_info "Installing Node.js via apt..."
            curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
            sudo apt-get install -y nodejs
        elif command -v yum &> /dev/null; then
            print_info "Installing Node.js via yum..."
            curl -fsSL https://rpm.nodesource.com/setup_lts.x | sudo bash -
            sudo yum install -y nodejs
        else
            print_error "Could not detect package manager. Please install Node.js manually."
            print_info "Download from: https://nodejs.org/"
            exit 1
        fi
        print_success "Node.js installed: $(node --version)"
    else
        print_success "Node.js already installed: $(node --version)"
    fi

    if ! command -v npm &> /dev/null; then
        print_error "npm not found. Please install npm manually."
        exit 1
    else
        print_success "npm installed: $(npm --version)"
    fi
}

# Install Rust development tools
install_rust_tools() {
    print_step "Installing Rust development tools..."

    print_info "Installing just (command runner)..."
    cargo install --locked just

    print_info "Installing bacon (background code checker)..."
    cargo install --locked bacon

    print_info "Installing cargo-watch (file watcher)..."
    cargo install --locked cargo-watch

    print_info "Installing cargo-expand (macro expansion)..."
    cargo install --locked cargo-expand

    print_info "Installing cargo-audit (security audit)..."
    cargo install --locked cargo-audit

    print_info "Installing cargo-llvm-cov (coverage)..."
    cargo install --locked cargo-llvm-cov

    print_success "All Rust tools installed"
}

# Install Buf (Protobuf tool)
install_buf() {
    print_step "Installing Buf (Protobuf tool)..."

    if ! command -v buf &> /dev/null; then
        print_info "Installing Buf..."
        npm install -g @bufbuild/buf
        print_success "Buf installed"
    else
        print_success "Buf already installed: $(buf --version)"
    fi
}

# Install global npm packages
install_npm_tools() {
    print_step "Installing global npm packages..."

    # Install Playwright
    print_info "Installing Playwright..."
    npm install -g @playwright/test
    playwright install

    # Install other useful tools
    print_info "Installing additional npm tools..."
    npm install -g prettier eslint typescript

    print_success "Global npm packages installed"
}

# Install Docker
install_docker() {
    print_step "Checking Docker installation..."

    if ! command -v docker &> /dev/null; then
        print_warning "Docker not found."

        if [ "$OS" == "linux" ]; then
            print_info "Installing Docker on Linux..."
            curl -fsSL https://get.docker.com -o get-docker.sh
            sudo sh get-docker.sh
            sudo usermod -aG docker $USER
            rm get-docker.sh
            print_success "Docker installed. Please log out and back in for group changes to take effect."
        elif [ "$OS" == "macos" ]; then
            print_info "Please install Docker Desktop for Mac from:"
            print_info "https://www.docker.com/products/docker-desktop"
            exit 1
        else
            print_error "Please install Docker manually from:"
            print_error "https://www.docker.com/get-started"
            exit 1
        fi
    else
        print_success "Docker already installed: $(docker --version)"
    fi

    # Install docker-compose
    if ! command -v docker-compose &> /dev/null; then
        print_info "Installing docker-compose..."
        curl -L "https://github.com/docker/compose/releases/latest/download/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
        chmod +x /usr/local/bin/docker-compose
        print_success "docker-compose installed"
    else
        print_success "docker-compose already installed: $(docker-compose --version)"
    fi
}

# Setup VS Code
setup_vscode() {
    print_step "Setting up VS Code (optional)..."

    if command -v code &> /dev/null; then
        print_info "Installing recommended VS Code extensions..."

        # List of extensions to install
        extensions=(
            "rust-lang.rust-analyzer"
            "vadimcn.vscode-lldb"
            "bungcip.better-toml"
            "serayuzgur.crates"
            "ms-vscode.vscode-typescript-next"
            "bradlc.vscode-tailwindcss"
            "dbaeumer.vscode-eslint"
            "esbenp.prettier-vscode"
            "ms-playwright.playwright"
            "humao.rest-client"
            "eamodio.gitlens"
            "github.copilot"
        )

        for ext in "${extensions[@]}"; do
            code --install-extension "$ext" 2>/dev/null || true
        done

        print_success "VS Code extensions installed"
    else
        print_info "VS Code not found. Skipping extension installation."
        print_info "You can install it from: https://code.visualstudio.com/"
    fi
}

# Install project dependencies
install_project_deps() {
    print_step "Installing project dependencies..."

    print_info "Installing Rust dependencies..."
    cargo fetch

    print_info "Installing Node.js dependencies for frontend..."
    cd web && npm install && cd ..

    print_success "Project dependencies installed"
}

# Setup environment file
setup_env() {
    print_step "Setting up environment file..."

    if [ ! -f ".env" ]; then
        if [ -f ".env.example" ]; then
            cp .env.example .env
            print_success "Environment file created from .env.example"
        else
            cat > .env << EOF
# Database Configuration
HODEI_DATABASE_URL=postgres://postgres:postgres@localhost:5432/hodei
POSTGRES_USER=postgres
POSTGRES_PASSWORD=postgres
POSTGRES_DB=hodei

# Development Mode
HODEI_DEV_MODE=1
HODEI_DOCKER_ENABLED=1

# Server Configuration
GRPC_PORT=50051
REST_PORT=8080
WEB_PORT=3000

# Logging
RUST_LOG=debug,hodei=trace,sqlx=warn

# Optional Services
PROMETHEUS_PORT=9090
GRAFANA_PORT=3000
PGADMIN_PORT=5050
MAILHOG_SMTP_PORT=1025
MAILHOG_WEB_PORT=8025
EOF
            print_success "Environment file created"
        fi
    else
        print_success "Environment file already exists"
    fi
}

# Build initial project
build_project() {
    print_step "Building project for the first time..."

    print_info "Checking Rust compilation..."
    cargo check --workspace

    print_info "Building frontend..."
    cd web && npm run build && cd ..

    print_success "Initial build complete"
}

# Run setup database
setup_database() {
    print_step "Setting up development database..."

    print_info "Starting PostgreSQL..."
    docker-compose -f docker-compose.dev.yml up -d postgres

    # Wait for database
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

    print_success "Database is ready"

    # Run migrations
    if [ -d "crates/infrastructure" ]; then
        print_info "Running database migrations..."
        cd crates/infrastructure && cargo run --bin migrate && cd ../..
        print_success "Migrations complete"
    fi
}

# Show completion message
show_completion() {
    print_header "Setup Complete! 🎉"

    echo -e "${GREEN}Your development environment is ready!${NC}\n"

    echo -e "${CYAN}Quick Start:${NC}"
    echo -e "  ${YELLOW}./dev.sh${NC}              Start full development environment"
    echo -e "  ${YELLOW}just dev${NC}              Alternative using just"
    echo -e "  ${YELLOW}just dev-db${NC}           Start database only"
    echo -e "  ${YELLOW}just dev-backend${NC}      Start backend with hot reload"
    echo -e "  ${YELLOW}just dev-frontend${NC}     Start frontend with HMR"
    echo -e ""

    echo -e "${CYAN}Useful Commands:${NC}"
    echo -e "  ${YELLOW}just test${NC}              Run all tests"
    echo -e "  ${YELLOW}just check${NC}             Lint and format code"
    echo -e "  ${YELLOW}just logs${NC}              View logs"
    echo -e "  ${YELLOW}./dev.sh --help${NC}        Show all available commands"
    echo -e ""

    echo -e "${CYAN}Development URLs:${NC}"
    echo -e "  Frontend:  ${GREEN}http://localhost:5173${NC}"
    echo -e "  Backend:   ${GREEN}http://localhost:50051${NC}"
    echo -e "  Database:  ${GREEN}localhost:5432${NC}"
    echo -e ""

    echo -e "${CYAN}Optional Services:${NC}"
    echo -e "  Prometheus: ${GREEN}http://localhost:9090${NC} (start with: ${YELLOW}just dev-prometheus${NC})"
    echo -e "  Grafana:    ${GREEN}http://localhost:3000${NC} (start with: ${YELLOW}just dev-grafana${NC})"
    echo -e "  pgAdmin:    ${GREEN}http://localhost:5050${NC} (start with: ${YELLOW}just dev-pgadmin${NC})"
    echo -e ""

    echo -e "${CYAN}Documentation:${NC}"
    echo -e "  README.md        - Project overview"
    echo -e "  GETTING_STARTED.md - Detailed setup guide"
    echo -e "  docs/           - Full documentation"
    echo -e ""

    print_success "Happy coding! 🚀"
}

# Show usage
show_usage() {
    cat << EOF
Hodei Job Platform - Setup Script

Usage: ./setup.sh [options]

Options:
  --minimal    Skip installation of optional tools (VS Code, monitoring, etc.)
  --help       Show this help message

Examples:
  ./setup.sh              # Full setup
  ./setup.sh --minimal    # Minimal setup

This script will install:
  - Rust toolchain and development tools
  - Node.js and npm
  - Docker and docker-compose
  - Buf (Protobuf tool)
  - All project dependencies

Time: 2-5 minutes depending on your internet connection.
EOF
}

# Main execution
main() {
    local minimal=false

    while [[ $# -gt 0 ]]; do
        case $1 in
            --minimal)
                minimal=true
                shift
                ;;
            --help|-h)
                show_usage
                exit 0
                ;;
            *)
                print_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
        esac
    done

    print_header "Hodei Job Platform - Development Setup"
    print_info "This will set up your complete development environment."
    print_info "Time required: 2-5 minutes\n"

    check_root
    detect_os

    install_rust
    install_nodejs

    if [ "$minimal" = false ]; then
        print_info "Minimal mode disabled. Installing all tools..."
    else
        print_info "Minimal mode enabled. Skipping optional tools..."
    fi

    install_rust_tools
    install_buf

    if [ "$minimal" = false ]; then
        install_npm_tools
        install_docker
        setup_vscode
    fi

    setup_env
    install_project_deps
    build_project

    if [ "$minimal" = false ]; then
        setup_database
    fi

    show_completion
}

# Run main function
main "$@"
