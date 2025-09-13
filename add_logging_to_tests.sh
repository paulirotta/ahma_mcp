#!/bin/bash

# Script to add logging initialization to all test functions in Rust test files

# First, let's add the common module import to files that don't have it
echo "Adding common module imports..."

# List of files that need common module import
for file in tests/*.rs; do
    # Skip common directory and any backup files
    if [[ "$file" == "tests/common"* ]] || [[ "$file" == *".backup" ]]; then
        continue
    fi
    
    # Check if file has tokio tests but no common module import
    if grep -q "#\[tokio::test\]" "$file" && ! grep -q "mod common;" "$file"; then
        echo "Adding common module import to $file"
        
        # Find the line number after the last use statement or the first non-comment line
        line_num=$(grep -n "^use " "$file" | tail -1 | cut -d: -f1)
        if [ -z "$line_num" ]; then
            line_num=$(grep -n "^[^/]" "$file" | head -1 | cut -d: -f1)
        fi
        
        # Insert the mod common line after the use statements
        if [ ! -z "$line_num" ]; then
            sed -i '' "${line_num}a\\
\\
mod common;\\
" "$file"
        fi
    fi
done

echo "Adding logging initialization to test functions..."

# Add logging initialization to each tokio test function
for file in tests/*.rs; do
    # Skip common directory and any backup files
    if [[ "$file" == "tests/common"* ]] || [[ "$file" == *".backup" ]]; then
        continue
    fi
    
    if grep -q "#\[tokio::test\]" "$file"; then
        echo "Processing $file"
        
        # Use sed to add logging initialization after each tokio test function declaration
        # This finds "#[tokio::test]" followed by "async fn" and adds the logging call after the opening brace
        sed -i '' '/#\[tokio::test\]/,/^async fn.*{$/ {
            /^async fn.*{$/ a\
\    common::test_utils::init_logging_for_tests();
        }' "$file"
    fi
done

echo "Done! Please review the changes and run cargo check to verify."