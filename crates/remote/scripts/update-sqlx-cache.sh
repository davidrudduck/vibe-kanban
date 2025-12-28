#!/usr/bin/env bash
#
# update-sqlx-cache.sh
#
# Updates the SQLx query cache (.sqlx/) for the remote crate.
# This is required after adding new SQL queries to enable offline compilation.
#
# Usage:
#   ./scripts/update-sqlx-cache.sh
#
# What it does:
#   1. Starts the PostgreSQL container (if not running)
#   2. Waits for database to be ready
#   3. Runs SQLx migrations
#   4. Generates the query cache with `cargo sqlx prepare`
#   5. Stops the database container (unless --keep-db is passed)
#
# The generated .sqlx/ files should be committed to git.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE_DIR="$(dirname "$SCRIPT_DIR")"
cd "$REMOTE_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() { echo -e "${GREEN}[update-sqlx-cache]${NC} $1"; }
warn() { echo -e "${YELLOW}[update-sqlx-cache]${NC} $1"; }
error() { echo -e "${RED}[update-sqlx-cache]${NC} $1" >&2; }

# Parse arguments
KEEP_DB=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --keep-db)
            KEEP_DB=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--keep-db]"
            echo ""
            echo "Options:"
            echo "  --keep-db     Don't stop the database container after running"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check for required tools
if ! command -v docker &> /dev/null; then
    error "docker is not installed"
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    error "cargo is not installed"
    exit 1
fi

# Check if sqlx-cli is installed
if ! cargo sqlx --version &> /dev/null; then
    warn "sqlx-cli not found, installing..."
    cargo install sqlx-cli --no-default-features --features postgres
fi

# Fixed credentials matching docker-compose.db.yml
# This uses a standalone container, not your production database
DATABASE_URL="postgres://postgres:postgres@localhost:5435/vibe_remote_dev"
export DATABASE_URL

log "Using DATABASE_URL: $DATABASE_URL"

log "Starting PostgreSQL container..."
docker compose -f docker-compose.db.yml up -d sqlx-db

log "Waiting for database to be ready..."
max_attempts=30
attempt=0
while ! docker compose -f docker-compose.db.yml exec -T sqlx-db pg_isready -U postgres -d vibe_remote_dev &> /dev/null; do
    attempt=$((attempt + 1))
    if [[ $attempt -ge $max_attempts ]]; then
        error "Database failed to become ready after $max_attempts attempts"
        exit 1
    fi
    echo -n "."
    sleep 1
done
echo ""
log "Database is ready"

log "Running migrations..."
cargo sqlx migrate run

log "Generating SQLx query cache..."
cargo sqlx prepare

log "Query cache updated successfully!"
echo ""
log "Files to commit:"
git status --short .sqlx/ 2>/dev/null || ls -la .sqlx/*.json | head -5

if [[ "$KEEP_DB" == "false" ]]; then
    log "Stopping database container..."
    docker compose -f docker-compose.db.yml down
else
    log "Database container left running (--keep-db)"
    log "Connection: $DATABASE_URL"
fi

echo ""
log "Done! Don't forget to commit the .sqlx/ changes:"
echo "  git add .sqlx/"
echo "  git commit -m 'chore: update SQLx query cache'"
