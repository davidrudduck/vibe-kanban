#!/usr/bin/env bash
#
# update-sqlx-cache.sh
#
# Regenerates the SQLx query cache (.sqlx/) for offline Docker builds.
# Connects to your running Hive database, runs migrations, and updates the cache.
#
# Usage:
#   ./scripts/update-sqlx-cache.sh
#
# Requirements:
#   - Hive database running: docker compose --env-file .env.remote up -d
#   - .env.remote with POSTGRES_USER, POSTGRES_PASSWORD, POSTGRES_DB
#   - sqlx-cli with postgres support:
#       cargo install sqlx-cli --no-default-features --features native-tls,postgres

set -euo pipefail
cd "$(dirname "$0")/.."

log() { echo "[sqlx] $1"; }
error() { echo "[sqlx] ERROR: $1" >&2; exit 1; }

# Check sqlx-cli is installed with postgres support
if ! cargo sqlx --version &> /dev/null; then
    error "sqlx-cli not found. Install with:
    cargo install sqlx-cli --no-default-features --features native-tls,postgres"
fi

# Load credentials from .env.remote
[[ -f ".env.remote" ]] || error ".env.remote not found"

POSTGRES_USER=$(grep -E "^POSTGRES_USER=" .env.remote | cut -d'=' -f2)
POSTGRES_PASSWORD=$(grep -E "^POSTGRES_PASSWORD=" .env.remote | cut -d'=' -f2)
POSTGRES_DB=$(grep -E "^POSTGRES_DB=" .env.remote | cut -d'=' -f2)
POSTGRES_PORT=$(grep -E "^POSTGRES_PORT=" .env.remote | cut -d'=' -f2 || echo "5434")
POSTGRES_PORT="${POSTGRES_PORT:-5434}"

[[ -n "$POSTGRES_USER" && -n "$POSTGRES_PASSWORD" && -n "$POSTGRES_DB" ]] || \
    error "Missing POSTGRES_USER, POSTGRES_PASSWORD, or POSTGRES_DB in .env.remote"

DB_URL="postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${POSTGRES_PORT}/${POSTGRES_DB}"
log "Connecting to localhost:${POSTGRES_PORT}/${POSTGRES_DB}"

# Run migrations and generate cache
log "Running migrations..."
cargo sqlx migrate run --database-url "$DB_URL"

log "Generating query cache..."
cargo sqlx prepare --database-url "$DB_URL"

log "Done! Files updated:"
git status --short .sqlx/ 2>/dev/null || ls .sqlx/*.json 2>/dev/null | wc -l | xargs -I{} echo "  {} cache files"
echo ""
echo "Commit with: git add .sqlx/ && git commit -m 'chore: update SQLx query cache'"
