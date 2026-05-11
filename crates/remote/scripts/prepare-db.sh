#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RELAY_TUNNEL_DIR="$(cd "$REMOTE_DIR/../../crates/relay-tunnel" && pwd)"

CHECK_MODE=""
for arg in "$@"; do
  case "$arg" in
    --check) CHECK_MODE="--check" ;;
  esac
done

# Check mode runs offline against the existing .sqlx cache.
if [ "$CHECK_MODE" = "--check" ]; then
  echo "➤ Checking SQLx data for remote (offline mode)..."
  SQLX_OFFLINE=true cargo sqlx prepare --check

  echo "➤ Checking SQLx data for relay-tunnel (offline mode)..."
  (cd "$RELAY_TUNNEL_DIR" && SQLX_OFFLINE=true cargo sqlx prepare --check)

  echo "✅ sqlx check complete"
  exit 0
fi

# Prepare mode needs a running PostgreSQL.
# Override REMOTE_PREPARE_DATABASE_URL to point at an existing instance,
# otherwise we spin up a disposable Postgres container via Docker.
if [ -n "${REMOTE_PREPARE_DATABASE_URL:-}" ]; then
  export DATABASE_URL="$REMOTE_PREPARE_DATABASE_URL"
  echo "➤ Using existing database: $DATABASE_URL"
  echo "➤ Running migrations..."
  sqlx migrate run
else
  command -v docker >/dev/null 2>&1 || {
    echo "❌ docker not found. Install Docker or set REMOTE_PREPARE_DATABASE_URL." >&2
    exit 1
  }

  PORT=54329
  CONTAINER_NAME="vibe-kanban-sqlx-prepare-$$"
  PG_PASSWORD="sqlxprepare"

  cleanup() {
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
  }
  trap cleanup EXIT

  echo "➤ Killing any container holding port $PORT..."
  existing=$(docker ps -q --filter "publish=$PORT" 2>/dev/null || true)
  [ -n "$existing" ] && docker rm -f $existing >/dev/null 2>&1 || true

  echo "➤ Starting disposable Postgres 17 in Docker on port $PORT..."
  docker run -d --rm \
    --name "$CONTAINER_NAME" \
    -e POSTGRES_PASSWORD="$PG_PASSWORD" \
    -e POSTGRES_DB=remote \
    -p "$PORT:5432" \
    postgres:17-alpine >/dev/null

  echo "➤ Waiting for Postgres to accept connections..."
  for i in $(seq 1 30); do
    if docker exec "$CONTAINER_NAME" pg_isready -U postgres -d remote >/dev/null 2>&1; then
      break
    fi
    sleep 1
    if [ "$i" = "30" ]; then
      echo "❌ Postgres did not become ready in 30s" >&2
      exit 1
    fi
  done

  export DATABASE_URL="postgres://postgres:$PG_PASSWORD@localhost:$PORT/remote"

  echo "➤ Running migrations..."
  sqlx migrate run
fi

echo "➤ Preparing SQLx data for remote..."
cargo sqlx prepare

echo "➤ Preparing SQLx data for relay-tunnel..."
(cd "$RELAY_TUNNEL_DIR" && cargo sqlx prepare)

echo "✅ sqlx prepare complete"
