#!/bin/bash
# Swarm Sync Reset - Run from any node
# Usage: ./run-reset.sh [hive|node|all]

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Load database path from .env if it exists
DB_PATH=""
if [[ -f "$PROJECT_ROOT/.env" ]]; then
  # Extract VK_DATABASE_PATH from .env (handles quotes and tilde expansion)
  DB_PATH=$(grep -E '^VK_DATABASE_PATH=' "$PROJECT_ROOT/.env" 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'")
  # Expand tilde
  DB_PATH="${DB_PATH/#\~/$HOME}"
fi

# Fallback to default paths if not set
if [[ -z "$DB_PATH" ]]; then
  if [[ "$(uname)" == "Darwin" ]]; then
    # macOS default
    DB_PATH="$HOME/Library/Application Support/vibe-kanban/db.sqlite"
  else
    # Linux default
    DB_PATH="$HOME/.vkswarm/db/db.sqlite"
  fi
fi

echo "Using database: $DB_PATH"

case "${1:-}" in
  hive)
    echo "=== Clearing Hive (postgres on TARDIS) ==="
    ssh tardis "docker exec -i remote-remote-db-1 psql -U remote -d remote" < "$SCRIPT_DIR/01-clear-hive.sql"
    echo "Done!"
    ;;

  node)
    echo "=== Clearing local node sync links ==="
    if [[ ! -f "$DB_PATH" ]]; then
      echo "ERROR: Database not found at $DB_PATH"
      echo "Set VK_DATABASE_PATH in $PROJECT_ROOT/.env or check the path."
      exit 1
    fi
    sqlite3 "$DB_PATH" < "$SCRIPT_DIR/02-clear-node-links.sql"
    echo "Done!"
    ;;

  dedupe)
    echo "=== Deduplicate Hive (keep originals with attempts) ==="
    ssh tardis "docker exec -i remote-remote-db-1 psql -U remote -d remote" < "$SCRIPT_DIR/03-dedupe-hive.sql"
    echo ""
    echo "Now run on each node: ./run-reset.sh fix-links"
    ;;

  fix-links)
    echo "=== Fix local task links after dedupe ==="
    if [[ ! -f "$DB_PATH" ]]; then
      echo "ERROR: Database not found at $DB_PATH"
      exit 1
    fi
    sqlite3 "$DB_PATH" < "$SCRIPT_DIR/04-fix-node-links.sql"
    echo "Done! Restart vibe-kanban to re-sync."
    ;;

  all)
    echo "=== Full Reset: Hive + This Node ==="
    echo ""
    echo "Step 1: Clearing Hive..."
    ssh tardis "docker exec -i remote-remote-db-1 psql -U remote -d remote" < "$SCRIPT_DIR/01-clear-hive.sql"
    echo ""
    echo "Step 2: Clearing local node..."
    if [[ ! -f "$DB_PATH" ]]; then
      echo "ERROR: Database not found at $DB_PATH"
      exit 1
    fi
    sqlite3 "$DB_PATH" < "$SCRIPT_DIR/02-clear-node-links.sql"
    echo ""
    echo "=== Reset Complete ==="
    echo "Now run this on other nodes: ./run-reset.sh node"
    echo "Then restart all vibe-kanban servers."
    ;;

  *)
    echo "Swarm Sync Reset Script"
    echo ""
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  hive       - Clear hive database only (run once from any node)"
    echo "  node       - Clear this node's sync links (run on each node)"
    echo "  all        - Clear hive + this node (run on first node, then 'node' on others)"
    echo "  dedupe     - Remove duplicate shared_tasks (keeps originals with attempts)"
    echo "  fix-links  - Fix local links after dedupe (run on each node)"
    echo ""
    echo "Database path: $DB_PATH"
    echo "(Set VK_DATABASE_PATH in .env to override)"
    echo ""
    echo "Full reset (nuclear option):"
    echo "  1. Stop all nodes"
    echo "  2. Run: ./run-reset.sh all    (on one node)"
    echo "  3. Run: ./run-reset.sh node   (on other nodes)"
    echo "  4. Restart all nodes"
    echo ""
    echo "Dedupe only (preserves data):"
    echo "  1. Stop all nodes"
    echo "  2. Run: ./run-reset.sh dedupe     (once, from any node)"
    echo "  3. Run: ./run-reset.sh fix-links  (on each node)"
    echo "  4. Restart all nodes"
    ;;
esac
