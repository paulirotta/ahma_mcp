#!/bin/bash

# Metrics Aggregator wrapper script
# This script runs the metrics-aggregator Rust tool on the repository root.

# Get the script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

cd "$SCRIPT_DIR/metrics-aggregator" || exit 1

# Run the aggregator on the repository root
cargo run -- "$REPO_ROOT"
