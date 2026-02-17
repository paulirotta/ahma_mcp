#!/bin/bash
# Test and commit current branch changes to git.
set -euo pipefail

cargo install --path ahma_mcp
cargo install --path ahma_simplify

echo "=== Install successful ==="
