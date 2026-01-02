#!/bin/bash
# Initialize and run the development environment for Vibe Kanban
# This script sets up environment configuration and starts development servers

set -e

# Configuration - these ports were selected as available during initialization
FRONTEND_PORT="${FRONTEND_PORT:-4000}"
BACKEND_PORT="${BACKEND_PORT:-4001}"
MCP_PORT="${MCP_PORT:-4002}"

echo "========================================"
echo "  Vibe Kanban Development Setup"
echo "========================================"
echo ""

# Step 1: Environment file setup
echo "1. Setting up environment configuration..."

# Check for .env.testing, copy from .env.example if missing
if [ ! -f .env.testing ]; then
    if [ -f .env.example ]; then
        echo "   Creating .env.testing from .env.example..."
        cp .env.example .env.testing
    else
        echo "   ERROR: No .env.example found to create .env.testing"
        exit 1
    fi
fi

# Copy .env.testing to .env (replacing if exists)
echo "   Copying .env.testing to .env..."
cp .env.testing .env

# Step 2: Update port configuration in .env
echo ""
echo "2. Configuring ports..."
echo "   FRONTEND_PORT=$FRONTEND_PORT"
echo "   BACKEND_PORT=$BACKEND_PORT"
echo "   MCP_PORT=$MCP_PORT"

# Update or add FRONTEND_PORT
if grep -q "^FRONTEND_PORT=" .env; then
    sed -i "s/^FRONTEND_PORT=.*/FRONTEND_PORT=$FRONTEND_PORT/" .env
elif grep -q "^# FRONTEND_PORT=" .env; then
    sed -i "s/^# FRONTEND_PORT=.*/FRONTEND_PORT=$FRONTEND_PORT/" .env
else
    echo "FRONTEND_PORT=$FRONTEND_PORT" >> .env
fi

# Update or add BACKEND_PORT
if grep -q "^BACKEND_PORT=" .env; then
    sed -i "s/^BACKEND_PORT=.*/BACKEND_PORT=$BACKEND_PORT/" .env
elif grep -q "^# BACKEND_PORT=" .env; then
    sed -i "s/^# BACKEND_PORT=.*/BACKEND_PORT=$BACKEND_PORT/" .env
else
    echo "BACKEND_PORT=$BACKEND_PORT" >> .env
fi

# Update or add MCP_PORT
if grep -q "^MCP_PORT=" .env; then
    sed -i "s/^MCP_PORT=.*/MCP_PORT=$MCP_PORT/" .env
elif grep -q "^# MCP_PORT=" .env; then
    sed -i "s/^# MCP_PORT=.*/MCP_PORT=$MCP_PORT/" .env
else
    echo "MCP_PORT=$MCP_PORT" >> .env
fi

# Step 3: Install dependencies
echo ""
echo "3. Installing dependencies..."
pnpm install

# Step 4: Display server information
echo ""
echo "========================================"
echo "  Setup Complete!"
echo "========================================"
echo ""
echo "Starting development servers..."
echo ""
echo "  Frontend: http://localhost:$FRONTEND_PORT"
echo "  Backend:  http://localhost:$BACKEND_PORT"
echo "  MCP:      http://localhost:$MCP_PORT/mcp"
echo ""
echo "Press Ctrl+C to stop the servers"
echo ""

# Step 5: Start development servers
# Export ports so they're available to the dev command
export FRONTEND_PORT
export BACKEND_PORT
export MCP_PORT

pnpm run dev
