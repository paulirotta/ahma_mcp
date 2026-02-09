#!/bin/bash

# Code Health Metrics wrapper script
# This script runs the ahma_code_health tool on the repository root.

# Get the script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

cd "$REPO_ROOT" || exit 1

# Run the code health aggregator on the repository root
cargo run -p ahma_code_health -- "$REPO_ROOT" --limit 20 --html --open
