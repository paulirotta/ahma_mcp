#!/bin/bash

echo "🔍 DEBUG: Running nextest with full debug logging to trace cancellation source"
echo "This will run nextest and show all cancellation-related debug logs"
echo

# Set debug logging for all AHMA MCP modules
export RUST_LOG="debug"

# Run a simple nextest command that's likely to get cancelled
echo "Starting nextest with debug logging..."
echo "Press Ctrl+C to trigger cancellation and see the debug trace"
echo

cargo run --bin ahma_mcp -- --tools-dir tools cargo_nextest nextest_run --workspace

echo
echo "🔍 DEBUG: Check the logs above for cancellation traces"
echo "Look for:"
echo "  - 'CANCELLATION DETECTED' messages"
echo "  - 'CANCEL_OPERATION_WITH_REASON' messages"  
echo "  - 'Attempting to cancel operation' messages"
echo "  - Process output containing 'Canceled: Canceled'"
