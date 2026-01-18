#!/usr/bin/env bash
# Clean cargo target directory to reclaim disk space
# Uses cargo-cache if available, falls back to manual cleanup

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_DIR="$PROJECT_ROOT/target"

# Parse arguments
AGGRESSIVE=false
DRY_RUN=false
INSTALL_CACHE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -a|--aggressive)
            AGGRESSIVE=true
            shift
            ;;
        -n|--dry-run)
            DRY_RUN=true
            shift
            ;;
        --install)
            INSTALL_CACHE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Clean cargo target directory to reclaim disk space."
            echo ""
            echo "Options:"
            echo "  -a, --aggressive  Remove more aggressively (incremental cache, all deps)"
            echo "  -n, --dry-run     Show what would be deleted without deleting"
            echo "  --install         Install cargo-cache if not present"
            echo "  -h, --help        Show this help message"
            echo ""
            echo "Without options, removes:"
            echo "  - Old incremental compilation artifacts"
            echo "  - Unused dependency artifacts"
            echo "  - Build script outputs"
            echo ""
            echo "With --aggressive, also removes:"
            echo "  - All incremental compilation cache"
            echo "  - All build artifacts (requires full rebuild)"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Function to get directory size
get_size() {
    if [[ -d "$1" ]]; then
        du -sh "$1" 2>/dev/null | cut -f1
    else
        echo "0"
    fi
}

# Function to remove with dry-run support
remove_dir() {
    local dir="$1"
    local desc="$2"
    if [[ -d "$dir" ]]; then
        local size=$(get_size "$dir")
        if [[ "$DRY_RUN" == "true" ]]; then
            echo -e "${YELLOW}[DRY RUN]${NC} Would remove $desc ($size): $dir"
        else
            echo -e "${GREEN}Removing${NC} $desc ($size): $dir"
            rm -rf "$dir"
        fi
    fi
}

echo "Cargo Cache Cleaner"
echo "==================="
echo "Project: $PROJECT_ROOT"
echo ""

# Check current size
if [[ -d "$TARGET_DIR" ]]; then
    BEFORE_SIZE=$(get_size "$TARGET_DIR")
    echo -e "Current target/ size: ${YELLOW}$BEFORE_SIZE${NC}"
    echo ""
else
    echo "No target/ directory found."
    exit 0
fi

# Check if cargo-cache is available
if command -v cargo-cache &> /dev/null; then
    echo "Using cargo-cache for cleanup..."
    echo ""

    if [[ "$DRY_RUN" == "true" ]]; then
        echo -e "${YELLOW}[DRY RUN]${NC} Would run: cargo cache --autoclean"
        if [[ "$AGGRESSIVE" == "true" ]]; then
            echo -e "${YELLOW}[DRY RUN]${NC} Would run: cargo cache --remove-dir all-ref-cache,git-repos"
        fi
    else
        # Run autoclean
        cargo cache --autoclean

        if [[ "$AGGRESSIVE" == "true" ]]; then
            # More aggressive cleanup
            cargo cache --remove-dir all-ref-cache,git-repos 2>/dev/null || true
        fi
    fi
else
    echo "cargo-cache not found, using manual cleanup..."
    echo ""

    if [[ "$INSTALL_CACHE" == "true" ]]; then
        echo "Installing cargo-cache..."
        cargo install cargo-cache
        echo ""
        echo "cargo-cache installed. Run this script again."
        exit 0
    fi

    # Manual cleanup

    # 1. Clean old incremental compilation artifacts
    # These are in target/{debug,release}/incremental/
    if [[ "$AGGRESSIVE" == "true" ]]; then
        remove_dir "$TARGET_DIR/debug/incremental" "debug incremental cache"
        remove_dir "$TARGET_DIR/release/incremental" "release incremental cache"
    else
        # Only remove incremental artifacts older than 7 days
        if [[ -d "$TARGET_DIR/debug/incremental" ]] && [[ "$DRY_RUN" != "true" ]]; then
            echo "Cleaning old incremental artifacts (>7 days)..."
            find "$TARGET_DIR/debug/incremental" -type d -mtime +7 -exec rm -rf {} + 2>/dev/null || true
        fi
        if [[ -d "$TARGET_DIR/release/incremental" ]] && [[ "$DRY_RUN" != "true" ]]; then
            find "$TARGET_DIR/release/incremental" -type d -mtime +7 -exec rm -rf {} + 2>/dev/null || true
        fi
    fi

    # 2. Clean build script outputs older than 7 days
    if [[ -d "$TARGET_DIR/debug/build" ]] && [[ "$DRY_RUN" != "true" ]]; then
        echo "Cleaning old build script outputs (>7 days)..."
        find "$TARGET_DIR/debug/build" -type d -mtime +7 -exec rm -rf {} + 2>/dev/null || true
    fi
    if [[ -d "$TARGET_DIR/release/build" ]] && [[ "$DRY_RUN" != "true" ]]; then
        find "$TARGET_DIR/release/build" -type d -mtime +7 -exec rm -rf {} + 2>/dev/null || true
    fi

    # 3. Clean .fingerprint directories (can be regenerated)
    if [[ "$AGGRESSIVE" == "true" ]]; then
        remove_dir "$TARGET_DIR/debug/.fingerprint" "debug fingerprints"
        remove_dir "$TARGET_DIR/release/.fingerprint" "release fingerprints"
    fi

    # 4. Clean deps directories in aggressive mode
    if [[ "$AGGRESSIVE" == "true" ]]; then
        remove_dir "$TARGET_DIR/debug/deps" "debug dependencies"
        remove_dir "$TARGET_DIR/release/deps" "release dependencies"
    fi

    echo ""
    echo -e "${YELLOW}Tip:${NC} Install cargo-cache for smarter cleanup:"
    echo "  cargo install cargo-cache"
    echo "  # Or run: $0 --install"
fi

# Show results
if [[ -d "$TARGET_DIR" ]]; then
    AFTER_SIZE=$(get_size "$TARGET_DIR")
    echo ""
    echo "========================================"
    echo -e "Before: ${YELLOW}$BEFORE_SIZE${NC}"
    echo -e "After:  ${GREEN}$AFTER_SIZE${NC}"
    echo "========================================"
fi

if [[ "$DRY_RUN" == "true" ]]; then
    echo ""
    echo -e "${YELLOW}This was a dry run. No files were deleted.${NC}"
    echo "Run without --dry-run to actually clean."
fi
