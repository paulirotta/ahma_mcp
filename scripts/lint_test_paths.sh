#!/bin/bash
# Lint script to detect improper CARGO_TARGET_DIR usage in tests
# This prevents bugs from duplicated path resolution logic

set -euo pipefail

echo "🔍 Checking for improper CARGO_TARGET_DIR usage in tests..."

# Find all Rust test files
VIOLATIONS=0

# Search for CARGO_TARGET_DIR outside of test_utils::cli
while IFS= read -r file; do
    # Skip the allowed file
    if [[ "$file" == *"ahma_mcp/src/test_utils.rs" ]]; then
        continue
    fi
    
    # Skip files that just remove the env var (that's OK)
    if grep -q 'std::env::var("CARGO_TARGET_DIR")' "$file" && ! grep -q 'env_remove("CARGO_TARGET_DIR")' "$file"; then
        echo "FAIL VIOLATION: $file"
        echo "   Found manual CARGO_TARGET_DIR access"
        echo "   Use ahma_mcp::test_utils::cli::get_binary_path() instead"
        echo ""
        VIOLATIONS=$((VIOLATIONS + 1))
    fi
done < <(find . -path "*/tests/*.rs" -o -name "*_test.rs" -o -name "test_*.rs" | grep -v target)

if [ $VIOLATIONS -eq 0 ]; then
    echo "OK No violations found"
    exit 0
else
    echo ""
    echo "FAIL Found $VIOLATIONS violation(s)"
    echo ""
    echo "Fix: Replace manual CARGO_TARGET_DIR logic with:"
    echo "  - ahma_mcp::test_utils::cli::get_binary_path(package, binary)"
    echo "  - ahma_mcp::test_utils::cli::build_binary_cached(package, binary)"
    exit 1
fi
