#!/bin/bash
# 
# Fetch the latest branches
echo "Checking code coverage..."
echo
cargo llvm-cov --json --output-path coverage.json
echo
echo "Code coverage report generated at coverage.json"
