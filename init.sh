#!/bin/bash
# Initialize and run the development environment for Vibe Kanban
# Usage:
#   ./init.sh          - Setup and start development servers
#   ./init.sh start    - Start development servers
#   ./init.sh stop     - Stop development servers gracefully
#   ./init.sh status   - Check server status

set -e

# Configuration
FRONTEND_PORT=${FRONTEND_PORT:-4000}
BACKEND_PORT=${BACKEND_PORT:-4001}
MCP_PORT=${MCP_PORT:-4002}
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

# Check if required tools are installed
check_prerequisites() {
    log_info "Checking prerequisites..."

    local missing=()

    if ! command -v pnpm &> /dev/null; then
        missing+=("pnpm")
    fi

    if ! command -v cargo &> /dev/null; then
        missing+=("cargo/rust")
    fi

    if ! command -v node &> /dev/null; then
        missing+=("node")
    fi

    if [ ${#missing[@]} -ne 0 ]; then
        log_error "Missing required tools: ${missing[*]}"
        echo ""
        echo "Please install the following:"
        echo "  - Rust: https://rustup.rs/"
        echo "  - Node.js: https://nodejs.org/ (>=18)"
        echo "  - pnpm: npm install -g pnpm"
        exit 1
    fi

    log_success "All prerequisites found"
}

# Setup environment file
setup_env() {
    log_info "Setting up environment..."

    if [ ! -f "$PROJECT_DIR/.env" ]; then
        if [ -f "$PROJECT_DIR/.env.example" ]; then
            log_info "Creating .env from .env.example..."
            cp "$PROJECT_DIR/.env.example" "$PROJECT_DIR/.env"
        else
            log_warn "No .env.example found, creating minimal .env..."
            touch "$PROJECT_DIR/.env"
        fi
    fi

    # Update FRONTEND_PORT in .env
    if grep -q "^FRONTEND_PORT=" "$PROJECT_DIR/.env"; then
        sed -i "s/^FRONTEND_PORT=.*/FRONTEND_PORT=$FRONTEND_PORT/" "$PROJECT_DIR/.env"
    else
        echo "FRONTEND_PORT=$FRONTEND_PORT" >> "$PROJECT_DIR/.env"
    fi

    # Update BACKEND_PORT in .env
    if grep -q "^BACKEND_PORT=" "$PROJECT_DIR/.env"; then
        sed -i "s/^BACKEND_PORT=.*/BACKEND_PORT=$BACKEND_PORT/" "$PROJECT_DIR/.env"
    else
        echo "BACKEND_PORT=$BACKEND_PORT" >> "$PROJECT_DIR/.env"
    fi

    log_success "Environment configured (FRONTEND_PORT=$FRONTEND_PORT, BACKEND_PORT=$BACKEND_PORT)"
}

# Install dependencies
install_deps() {
    log_info "Installing dependencies..."

    cd "$PROJECT_DIR"

    # Install Node.js dependencies
    if [ ! -d "node_modules" ] || [ ! -d "frontend/node_modules" ]; then
        log_info "Installing pnpm dependencies..."
        pnpm install
    else
        log_info "Dependencies already installed, skipping..."
    fi

    log_success "Dependencies installed"
}

# Check if ports are available
check_ports() {
    local port_in_use=0

    if lsof -i :$FRONTEND_PORT > /dev/null 2>&1; then
        log_warn "Port $FRONTEND_PORT (frontend) is already in use"
        port_in_use=1
    fi

    if lsof -i :$BACKEND_PORT > /dev/null 2>&1; then
        log_warn "Port $BACKEND_PORT (backend) is already in use"
        port_in_use=1
    fi

    if [ $port_in_use -eq 1 ]; then
        log_info "Currently used ports:"
        lsof -i -P -n | grep "LISTEN" | head -20
        echo ""
        log_error "Please stop the conflicting services or change ports in .env"
        return 1
    fi

    return 0
}

# Start development servers
start_servers() {
    log_info "Starting development servers..."

    cd "$PROJECT_DIR"

    # Check if ports are available
    if ! check_ports; then
        exit 1
    fi

    echo ""
    log_info "Starting Vibe Kanban development server..."
    echo ""
    echo -e "${GREEN}================================================${NC}"
    echo -e "${GREEN}  Vibe Kanban Development Environment${NC}"
    echo -e "${GREEN}================================================${NC}"
    echo ""
    echo -e "  Frontend: ${BLUE}http://localhost:$FRONTEND_PORT${NC}"
    echo -e "  Backend:  ${BLUE}http://localhost:$BACKEND_PORT${NC}"
    echo ""
    echo -e "  To stop: ${YELLOW}./init.sh stop${NC} or ${YELLOW}pnpm run stop${NC}"
    echo ""
    echo -e "${GREEN}================================================${NC}"
    echo ""

    # Export ports for pnpm dev
    export FRONTEND_PORT
    export BACKEND_PORT

    # Start using pnpm dev
    pnpm run dev
}

# Stop development servers
stop_servers() {
    log_info "Stopping development servers..."

    cd "$PROJECT_DIR"

    # Use the built-in stop command
    pnpm run stop 2>/dev/null || true

    log_success "Servers stopped"
}

# Check status of servers
check_status() {
    log_info "Checking server status..."

    cd "$PROJECT_DIR"

    # Check for running instances
    pnpm run stop --list 2>/dev/null || true

    echo ""
    log_info "Ports in use:"
    lsof -i -P -n | grep "LISTEN" | head -20 || echo "No ports in use"
}

# Main setup function
setup() {
    echo ""
    echo -e "${BLUE}================================================${NC}"
    echo -e "${BLUE}  Vibe Kanban Development Setup${NC}"
    echo -e "${BLUE}================================================${NC}"
    echo ""

    check_prerequisites
    setup_env
    install_deps

    echo ""
    log_success "Setup complete!"
    echo ""
    log_info "To start the development servers, run:"
    echo -e "  ${YELLOW}./init.sh start${NC}"
    echo ""
}

# Parse command line arguments
case "${1:-setup}" in
    start)
        start_servers
        ;;
    stop)
        stop_servers
        ;;
    status)
        check_status
        ;;
    setup|"")
        setup
        ;;
    *)
        echo "Usage: $0 {setup|start|stop|status}"
        echo ""
        echo "Commands:"
        echo "  setup   - Initial setup (default): check prerequisites, create .env, install deps"
        echo "  start   - Start development servers"
        echo "  stop    - Stop development servers gracefully"
        echo "  status  - Check server status and port usage"
        exit 1
        ;;
esac
