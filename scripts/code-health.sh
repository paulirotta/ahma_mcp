#!/bin/bash

# Code Health Metrics wrapper script
#
# This script runs the ahma_code_health tool to analyze code health metrics
# for any directory (not just the ahma_mcp repository).
#
# Usage:
#   ./scripts/code-health.sh [TARGET_DIR] [ADDITIONAL_ARGS...]
#
# Arguments:
#   TARGET_DIR        - Directory to analyze (optional, defaults to current directory)
#   ADDITIONAL_ARGS   - Additional arguments passed to ahma_code_health tool
#
# Examples:
#   ./scripts/code-health.sh                    # Analyze current directory
#   ./scripts/code-health.sh /path/to/project   # Analyze specific directory
#   ./scripts/code-health.sh . --limit 10       # Analyze current dir, show top 10 issues
#
# Note: This script can be called from any directory. It will find the ahma_mcp
# repository root (where Cargo.toml is located) to run cargo, but the analysis
# target can be any directory on your system.

# Find the ahma_mcp repository root based on script location
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
AHMA_REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

# Verify we found the correct repository root
if [ ! -f "$AHMA_REPO_ROOT/Cargo.toml" ]; then
    echo "Error: Could not find ahma_mcp repository root (Cargo.toml not found)" >&2
    echo "Expected location: $AHMA_REPO_ROOT/Cargo.toml" >&2
    exit 1
fi

# Capture the original working directory for output files
ORIGINAL_CWD="$PWD"

# Target directory: first argument or current working directory
TARGET_DIR="${1:-$PWD}"

# Validate target directory exists
if [ ! -d "$TARGET_DIR" ]; then
    echo "Error: Target directory does not exist: $TARGET_DIR" >&2
    exit 1
fi

# Canonicalize target directory to absolute path
TARGET_DIR="$(cd "$TARGET_DIR" && pwd)" || {
    echo "Error: Could not access target directory: $TARGET_DIR" >&2
    exit 1
}

# Shift to pass remaining arguments to ahma_code_health
shift || true

echo "Analyzing: $TARGET_DIR"
echo "Running from ahma_mcp repo: $AHMA_REPO_ROOT"
echo ""

# Change to ahma_mcp repo root to run cargo, but analyze the target directory
cd "$AHMA_REPO_ROOT" || exit 1

# Run the code health aggregator on the target directory
# Use --output-path to write files to the original working directory
cargo run -p ahma_code_health -- "$TARGET_DIR" --html --open --output-path "$ORIGINAL_CWD" "$@"
