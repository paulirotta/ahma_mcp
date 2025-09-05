#!/usr/bin/env bash
# Test Quality Audit Script
set -euo pipefail

cargo llvm-cov nextest --html --open
cargo llvm-cov report --summary-only
