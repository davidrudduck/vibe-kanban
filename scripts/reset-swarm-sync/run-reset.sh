#!/bin/bash
# Swarm Sync Reset - Run from any node
# Usage: ./run-reset.sh [hive|node|all]

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

case "${1:-}" in
  hive)
    echo "=== Clearing Hive (postgres on TARDIS) ==="
    ssh tardis "docker exec -i remote-remote-db-1 psql -U remote -d remote" < "$SCRIPT_DIR/01-clear-hive.sql"
    echo "Done!"
    ;;

  node)
    echo "=== Clearing local node sync links ==="
    sqlite3 /home/david/.vkswarm/db/db.sqlite < "$SCRIPT_DIR/02-clear-node-links.sql"
    echo "Done!"
    ;;

  all)
    echo "=== Full Reset: Hive + This Node ==="
    echo ""
    echo "Step 1: Clearing Hive..."
    ssh tardis "docker exec -i remote-remote-db-1 psql -U remote -d remote" < "$SCRIPT_DIR/01-clear-hive.sql"
    echo ""
    echo "Step 2: Clearing local node..."
    sqlite3 /home/david/.vkswarm/db/db.sqlite < "$SCRIPT_DIR/02-clear-node-links.sql"
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
    echo "  hive  - Clear hive database only (run once from any node)"
    echo "  node  - Clear this node's sync links (run on each node)"
    echo "  all   - Clear hive + this node (run on first node, then 'node' on others)"
    echo ""
    echo "Recommended order:"
    echo "  1. Stop all nodes"
    echo "  2. Run: ./run-reset.sh all    (on one node)"
    echo "  3. Run: ./run-reset.sh node   (on other nodes)"
    echo "  4. Restart all nodes"
    ;;
esac
