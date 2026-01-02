#!/bin/bash
# Initialize and run the development environment for Vibe Kanban
# This script sets up the environment and starts development servers

set -e

# Port configuration - these match the task variables set in VKSwarm
FRONTEND_PORT="${FRONTEND_PORT:-4000}"
BACKEND_PORT="${BACKEND_PORT:-4001}"
MCP_PORT="${MCP_PORT:-4002}"

echo "Setting up Vibe Kanban development environment..."
echo ""

# Step 1: Check for .env.testing file, create from .env.example if missing
if [ ! -f .env.testing ]; then
    if [ -f .env.example ]; then
        echo "Creating .env.testing from .env.example..."
        cp .env.example .env.testing
    else
        echo "ERROR: .env.example not found. Cannot create .env.testing"
        exit 1
    fi
fi

# Step 2: Copy .env.testing to .env (replacing if exists)
echo "Copying .env.testing to .env..."
cp .env.testing .env

# Step 3: Update port configuration in .env
echo "Configuring ports (FRONTEND: $FRONTEND_PORT, BACKEND: $BACKEND_PORT, MCP: $MCP_PORT)..."

# Function to update or add a variable in .env
update_env_var() {
    local key="$1"
    local value="$2"
    local file=".env"

    if grep -q "^${key}=" "$file" 2>/dev/null; then
        # Update existing variable
        sed -i "s|^${key}=.*|${key}=${value}|" "$file"
    elif grep -q "^# *${key}=" "$file" 2>/dev/null; then
        # Uncomment and update commented variable
        sed -i "s|^# *${key}=.*|${key}=${value}|" "$file"
    else
        # Add new variable
        echo "${key}=${value}" >> "$file"
    fi
}

update_env_var "FRONTEND_PORT" "$FRONTEND_PORT"
update_env_var "BACKEND_PORT" "$BACKEND_PORT"
update_env_var "MCP_PORT" "$MCP_PORT"
update_env_var "HOST" "127.0.0.1"

# Step 4: Install dependencies
echo ""
echo "Installing dependencies..."

# Check if pnpm is installed
if ! command -v pnpm &> /dev/null; then
    echo "ERROR: pnpm is required but not installed."
    echo "Install it with: npm install -g pnpm"
    exit 1
fi

# Install root dependencies
pnpm install

# Install frontend dependencies
cd frontend
pnpm install
cd ..

echo ""
echo "Dependencies installed successfully!"

# Step 5: Start development servers
echo ""
echo "========================================"
echo "Starting development servers..."
echo "========================================"
echo ""
echo "Frontend: http://localhost:$FRONTEND_PORT"
echo "Backend:  http://localhost:$BACKEND_PORT"
echo "MCP:      http://localhost:$MCP_PORT"
echo ""
echo "Press Ctrl+C to stop the servers"
echo ""

# Start the dev server using pnpm
# The dev script handles both frontend and backend concurrently
export FRONTEND_PORT
export BACKEND_PORT
export MCP_PORT
pnpm run dev
