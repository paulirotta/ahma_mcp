#!/bin/bash
cd "$(dirname "$0")"

echo "Starting ahma_mcp MCP server test..." >&2

# Test 1: Check if binary runs with help
echo "Test 1: Binary help output" >&2
./target/release/ahma_mcp --help >/dev/null && echo "✓ Binary executable" >&2 || echo "✗ Binary not working" >&2

# Test 2: Check if it can start and load configurations
echo "Test 2: Configuration loading" >&2
timeout_cmd() {
    perl -e 'alarm shift; exec @ARGV' "$@"
}

(timeout_cmd 3 ./target/release/ahma_mcp --debug 2>&1 | head -5) & 
sleep 0.5
kill $! 2>/dev/null || true
wait $! 2>/dev/null || true

echo "Basic tests completed" >&2
