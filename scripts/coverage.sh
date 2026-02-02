#!/bin/bash
# 
# Fetch the latest branches
echo "Checking code coverage..."
echo
cargo llvm-cov --html --output-path coverage.html
echo
echo "Code coverage report generated at coverage.json"
