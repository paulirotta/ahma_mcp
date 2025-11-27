#!/bin/bash
# MCP Inspector Script
#
# Launch ahma_mcp as an interactive local web servicer using the MCP Inspector tool.
# Use this to test and debug your MCP tool definitions.
# Usage: ./scripts/mcp-inspector.sh
set -euo pipefail

#TODO Pass in the sandbox scope directory as a command line argument based on the current working directory when calling this script
# Example:
# cd /Users/phoughton/github/ahma_mcp/ahma_shell
# ../scripts/ahma_mcp_http_server.sh
# This will pass in the sandbox scope directory as /Users/phoughton/github/ahma_mcp/ahma_shell
# This will then be used to create the sandbox scope directory and enforce the sandbox rules so command line tools like cargo_build will work as expected and no command line tools will be able to access the file system outside of the sandbox scope directory

# Resolve script directory and project root (assumes script lives in <project>/scripts/)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

#TODO (cd "$PROJECT_ROOT" && cargo run --bin ahma_mcp -- --async --mode http --http-port 3000 --tools-dir "$PROJECT_ROOT/.ahma/tools" --sandbox-scope "$PROJECT_ROOT")
(cd "$PROJECT_ROOT" && cargo run --bin ahma_mcp -- --async --mode http --http-port 3000 --tools-dir "$PROJECT_ROOT/.ahma/tools")
