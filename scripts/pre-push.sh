#!/bin/bash
# Pre-push guardrail to prevent CI-only workspace failures.
#
# Install:
#   cp scripts/pre-push.sh .git/hooks/pre-push && chmod +x .git/hooks/pre-push

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "Running pre-push checks..."

DIRTY_STATUS="$(git status --porcelain)"
if [ -n "$DIRTY_STATUS" ]; then
  echo "FAIL Refusing push: working tree is not clean."
  echo ""
  echo "Detected changes:"
  echo "$DIRTY_STATUS"
  echo ""
  echo "Commit/stash/remove local changes (including untracked files) and retry."
  exit 1
fi

echo "OK Working tree is clean"
echo "=== Running cargo check --workspace --locked ==="
cargo check --workspace --locked

echo "OK Pre-push checks passed"
