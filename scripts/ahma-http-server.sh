#!/bin/bash
# Ahma HTTP Server Script
#
# Launch ahma_mcp as an HTTP/SSE server for MCP clients.
#
# Usage:
#   ./scripts/ahma-http-server.sh                    # Use current directory as sandbox
#   ./scripts/ahma-http-server.sh /path/to/project   # Use specified directory as sandbox
#   ./scripts/ahma-http-server.sh --release          # Build in release mode
#   ./scripts/ahma-http-server.sh --release /path    # Release mode with custom sandbox
#   AHMA_SANDBOX_SCOPE=/path/to/project ./scripts/ahma-http-server.sh  # Via env var
#
# The sandbox scope determines which directory the AI can access.
# For security, the sandbox is set once at startup and cannot be changed.
set -euo pipefail

# Resolve script directory and project root (assumes script lives in <project>/scripts/)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TOOLS_DIR="$PROJECT_ROOT/.ahma/tools"

# Parse arguments
RELEASE_FLAG=""
SANDBOX_ARG=""

for arg in "$@"; do
    case "$arg" in
        --release)
            RELEASE_FLAG="--release"
            ;;
        *)
            if [[ -z "$SANDBOX_ARG" ]]; then
                SANDBOX_ARG="$arg"
            fi
            ;;
    esac
done

# Determine sandbox scope (priority: arg > AHMA_SANDBOX_SCOPE env > $PWD)
if [[ -n "$SANDBOX_ARG" ]]; then
    SANDBOX_SCOPE="$(cd "$SANDBOX_ARG" && pwd)"
    echo "Sandbox scope from argument: $SANDBOX_SCOPE"
elif [[ -n "${AHMA_SANDBOX_SCOPE:-}" ]]; then
    SANDBOX_SCOPE="$(cd "$AHMA_SANDBOX_SCOPE" && pwd)"
    echo "Sandbox scope from AHMA_SANDBOX_SCOPE: $SANDBOX_SCOPE"
else
    SANDBOX_SCOPE="$(pwd)"
    echo "Sandbox scope from current directory: $SANDBOX_SCOPE"
fi

echo
echo "Starting ahma_mcp HTTP server..."
echo "  Tools dir:     $TOOLS_DIR"
echo "  Sandbox scope: $SANDBOX_SCOPE"
echo "  Port:          3000"
if [[ -n "$RELEASE_FLAG" ]]; then
    echo "  Build mode:    Release (optimized)"
fi
echo "-----------------------------------------------"
echo "⚠️  Security: HTTP mode is for local development only."
echo "    Do not expose to untrusted networks."
echo "-----------------------------------------------"

(cd "$PROJECT_ROOT" && cargo run $RELEASE_FLAG -p ahma_core --bin ahma_mcp -- \
    --mode http \
    --http-port 3000 \
    --tools-dir "$TOOLS_DIR" \
    --sandbox-scope "$SANDBOX_SCOPE" \
)
