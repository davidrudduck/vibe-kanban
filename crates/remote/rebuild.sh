#!/bin/bash
# Rebuild script for vibe-kanban remote/hive server
#
# Usage:
#   ./rebuild.sh          # Rebuild only remote-server container (fast)
#   ./rebuild.sh --full   # Rebuild all containers (slower, use when dependencies change)

set -e

cd "$(dirname "$0")"

# Get git info for build
export VK_GIT_COMMIT=$(git rev-parse --short HEAD)
export VK_GIT_BRANCH=$(git branch --show-current)

echo "Building with commit: $VK_GIT_COMMIT, branch: $VK_GIT_BRANCH"

if [[ "$1" == "--full" ]]; then
    echo "Full rebuild: all containers..."
    docker compose --env-file .env.remote build --no-cache
else
    echo "Quick rebuild: remote-server only..."
    docker compose --env-file .env.remote build --no-cache remote-server
fi

echo "Starting containers..."
docker compose --env-file .env.remote up -d

echo "Done! Checking health..."
sleep 3
curl -s http://localhost:3000/v1/health | jq . || echo "Health check pending..."
