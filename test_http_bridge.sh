#!/bin/bash
# Test script for the HTTP bridge

set -e

echo "🧪 Testing Ahma HTTP Bridge"
echo "=============================="
echo

# Start the HTTP bridge in the background
echo "Starting HTTP bridge on port 3000..."
./target/release/ahma_mcp --mode http --http-port 3000 --tools-dir .ahma/tools &
SERVER_PID=$!

# Give the server time to start
echo "Waiting for server to start..."
sleep 3

# Function to cleanup on exit
cleanup() {
    echo
    echo "Stopping server..."
    kill $SERVER_PID 2>/dev/null || true
    wait $SERVER_PID 2>/dev/null || true
    echo "✅ Cleanup complete"
}
trap cleanup EXIT

# Test health endpoint
echo
echo "Testing health endpoint..."
HEALTH_RESPONSE=$(curl -s http://localhost:3000/health)
if [ "$HEALTH_RESPONSE" = "OK" ]; then
    echo "✅ Health check passed"
else
    echo "❌ Health check failed: $HEALTH_RESPONSE"
    exit 1
fi

# Test MCP initialize request
echo
echo "Testing MCP initialize request..."
INIT_REQUEST='{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {
      "name": "test-client",
      "version": "1.0.0"
    }
  }
}'

INIT_RESPONSE=$(curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d "$INIT_REQUEST")

echo "Response: $INIT_RESPONSE"

if echo "$INIT_RESPONSE" | grep -q '"protocolVersion"'; then
    echo "✅ Initialize request successful"
else
    echo "❌ Initialize request failed"
    exit 1
fi

# Test tools/list request
echo
echo "Testing tools/list request..."
LIST_REQUEST='{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}'

LIST_RESPONSE=$(curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d "$LIST_REQUEST")

echo "Response (truncated): $(echo "$LIST_RESPONSE" | head -c 200)..."

if echo "$LIST_RESPONSE" | grep -q '"tools"'; then
    echo "✅ Tools list request successful"
else
    echo "❌ Tools list request failed"
    exit 1
fi

echo
echo "=============================="
echo "✅ All tests passed!"
echo "=============================="

