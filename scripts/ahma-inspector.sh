#!/bin/bash
# MCP Inspector Script
#
# Launch ahma_mcp as an interactive local web servicer using the MCP Inspector tool.
# Use this to test and debug your MCP tool definitions.
# Usage: ./scripts/mcp-inspector.sh
set -euo pipefail

# Resolve script directory and project root (assumes script lives in <project>/scripts/)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Starting MCP Inspector from script dir: $SCRIPT_DIR"
echo "Project root detected as: $PROJECT_ROOT"

# Kill any existing MCP inspector processes
echo "Checking for existing MCP inspector processes..."
if pgrep -f "@modelcontextprotocol/inspector" > /dev/null; then
    echo "Killing existing MCP inspector processes..."
    pkill -f "@modelcontextprotocol/inspector" || true
    sleep 2
    echo "Existing processes terminated."
else
    echo "No existing MCP inspector processes found."
fi

# Build the project in release mode from project root
cd "$PROJECT_ROOT"
echo "Building Rust project with cargo build --release..."
cargo build --release

# Path to the built binary
BIN="$PROJECT_ROOT/target/release/ahma_mcp"

if [ -x "$BIN" ]; then
    echo "Build successful! Launching MCP Inspector..."
    echo "You can now interact directly with your MCP server tools."
    npx @modelcontextprotocol/inspector "$BIN"
else
    echo "Build failed or binary missing: $BIN"
    exit 1
fi
