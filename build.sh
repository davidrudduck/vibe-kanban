#!/bin/bash

# Get git info for build
export VK_GIT_COMMIT=$(git rev-parse --short HEAD)
export VK_GIT_BRANCH=$(git branch --show-current)

echo "Building with commit: $VK_GIT_COMMIT, branch: $VK_GIT_BRANCH"

if [[ "$1" == "--full" ]]; then
    echo "Full rebuild..."
    cargo clean
else
    echo "Quick rebuild.. "
fi

cargo build
rm -rf node_modules
rm -rf frontend/node_modules
pnpm install

