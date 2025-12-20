#!/bin/bash
# Ahma HTTP Server Script
#
# Launch ahma_http_bridge as an HTTP/SSE server for MCP clients.
# Uses session isolation mode by default: each connected IDE gets its own
# sandbox scope derived from its workspace roots.
#
# Usage:
#   ./scripts/ahma-http-server.sh                         # Session isolation (default)
#   ./scripts/ahma-http-server.sh --no-session-isolation  # Single shared process
#   ./scripts/ahma-http-server.sh --bind-addr 0.0.0.0:3000
#
# With session isolation enabled (default), the sandbox is determined per-client
# from the IDE's workspace roots. This allows multiple projects to safely share
# the same HTTP endpoint.
set -euo pipefail

# Resolve script directory and project root (assumes script lives in <project>/scripts/)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Build if needed
if [ ! -f "target/debug/ahma_http_bridge" ] || [ ! -f "target/debug/ahma_mcp" ]; then
    echo "Building ahma_http_bridge and ahma_mcp..."
    cargo build --bin ahma_http_bridge --bin ahma_mcp
fi

echo
echo "Starting Ahma HTTP Bridge..."
echo "  Port:              3000"
echo "  Session isolation: enabled (each client gets own sandbox)"
echo "-----------------------------------------------"
echo "⚠️  Security: HTTP mode is for local development only."
echo "    Do not expose to untrusted networks."
echo "-----------------------------------------------"
echo

exec cargo run --bin ahma_http_bridge -- "$@"

