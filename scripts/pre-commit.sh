#!/bin/bash
# Pre-commit hook to prevent CARGO_TARGET_DIR bugs
#
# Install: cp scripts/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit

set -e

echo "Running pre-commit checks..."

# Run the lint script
if ! ./scripts/lint_test_paths.sh; then
    echo ""
    echo "FAIL Pre-commit check failed"
    echo "Fix the violations above before committing"
    exit 1
fi

echo "OK Pre-commit checks passed"
