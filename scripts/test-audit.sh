#!/usr/bin/env bash
# Simplified Test Quality Audit Script
# Produces a compact markdown report about tests, perf, and coverage.
set -euo pipefail

# --- Helpers -----------------------------------------------------------------
readonly START_TIME=$(date +%s)
has_cmd() { command -v "$1" >/dev/null 2>&1; }
mktemp_file() { mktemp "${TMPDIR:-/tmp}/test-audit.XXXXXX"; }

# Colors only if stdout is a tty
if [ -t 1 ]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BLUE=''; NC=''
fi

log()    { printf "%s\n" "${BLUE}[AUDIT]${NC} $*"; }
success(){ printf "%s\n" "${GREEN}[SUCCESS]${NC} $*"; }
warning(){ printf "%s\n" "${YELLOW}[WARNING]${NC} $*"; }
error()  { printf "%s\n" "${RED}[ERROR]${NC} $*" >&2; }

# --- Workspace ---------------------------------------------------------------
if git_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"; then
    WORKSPACE_ROOT="${git_root:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
else
    WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi
REPORT_DIR="${WORKSPACE_ROOT}/audit-reports"
mkdir -p "$REPORT_DIR"
TIMESTAMP=$(date +"%Y%m%d-%H%M%S")
REPORT_FILE="${REPORT_DIR}/test-audit-${TIMESTAMP}.md"

# Temp files + cleanup
TMP_TIMING_LOG="$(mktemp_file)"
trap 'rm -f "$TMP_TIMING_LOG"' EXIT

# --- Tool selection ----------------------------------------------------------
# prefer `cargo nextest` if cargo is available; otherwise look for cargo-nextest binary
nextest_cmd=""
if has_cmd cargo && cargo nextest --version >/dev/null 2>&1; then
    nextest_cmd="cargo nextest"
elif has_cmd cargo-nextest; then
    nextest_cmd="cargo-nextest"
fi

llvm_cov_available=false
if has_cmd cargo-llvm-cov || (has_cmd cargo && cargo --list 2>/dev/null | grep -q llvm-cov); then
    llvm_cov_available=true
fi

# --- Precompute totals ------------------------------------------------------
TOTAL_TESTS=0
if [ -n "$nextest_cmd" ]; then
    # list can fail; guard it
    TOTAL_TESTS=$($nextest_cmd list --workspace 2>/dev/null | awk '/\btest\b/{count++}END{print count+0}')
fi

# --- Report header ----------------------------------------------------------
cat > "$REPORT_FILE" <<EOF
# Test Quality Audit Report

**Generated:** $(date +"%Y-%m-%d %H:%M:%S")
**Workspace:** $WORKSPACE_ROOT
**Git Branch:** $(git branch --show-current 2>/dev/null || echo "unknown")
**Git Commit:** $(git rev-parse --short HEAD 2>/dev/null || echo "unknown")

## Executive Summary

EOF

log "Starting test quality audit..."

# --- Test Count Analysis ----------------------------------------------------
log "Analyzing test count and distribution..."
{
    echo "## Test Count Analysis"
    echo
    echo "### Tests per Crate"
    echo '```'
    if [ -n "$nextest_cmd" ]; then
        # nextest list format varies; attempt to extract crate/name heuristically
        $nextest_cmd list --workspace 2>/dev/null \
            | sed -n 's/^[[:space:]]*//p' \
            | awk '/\btest\b/ {print $1}' \
            | sed 's/:.*$//' \
            | sort | uniq -c | sort -rn || true
    else
        echo "nextest not available"
    fi
    echo '```'
    echo
    echo "**Total Tests:** $TOTAL_TESTS"
    echo
} >> "$REPORT_FILE"

# --- Performance Analysis ---------------------------------------------------
log "Running performance analysis (short run, timed)..."
{
    echo "## Test Performance Analysis"
    echo
    echo "### Execution Time"
    echo '```'

    if [ -n "$nextest_cmd" ] && has_cmd timeout; then
        log "Invoking nextest with a 5m timeout..."
        if timeout 300 $nextest_cmd run --workspace --no-fail-fast 2>&1 | tee "$TMP_TIMING_LOG"; then
            # print a few relevant lines
            grep -E "(PASS|FAIL).*\[[0-9]+(\.[0-9]+)?s\]" "$TMP_TIMING_LOG" | head -20 || true
            echo
            echo "Summary:"
            grep -E "Summary|total" "$TMP_TIMING_LOG" | head -2 || true

            echo
            echo "Slow tests (>10s):"
            # extract seconds and filter numerically using awk
            awk '/(PASS|FAIL)/ {
                        if (match($0, /\[([0-9]+(\.[0-9]+)?)s\]/, m)) {
                            if (m[1] + 0 > 10) print $0
                        }
                    }' "$TMP_TIMING_LOG" || echo "No slow tests found"
        else
            echo "Performance run timed out or failed"
        fi
    else
        echo "Skipping performance run (nextest or timeout not available)"
    fi

    echo '```'
    echo
} >> "$REPORT_FILE"

# --- Coverage Analysis -----------------------------------------------------
log "Generating coverage analysis..."
{
    echo "## Coverage Analysis"
    echo

    if $llvm_cov_available; then
        echo "### Overall Coverage"
        echo '```'
        # best-effort, keep timeout
        if has_cmd timeout; then
            log "Running cargo llvm-cov (may take a while)..."
            if timeout 600 cargo llvm-cov nextest --workspace --all-features --summary-only 2>&1 | tee /dev/stderr | sed -n '1,200p'; then
                echo
                echo "Coverage generation completed (summary above)."
            else
                echo "Coverage generation timed out or failed."
            fi
        else
            # try without timeout
            if cargo llvm-cov nextest --workspace --all-features --summary-only 2>&1 | sed -n '1,200p'; then
                echo
                echo "Coverage generation completed (summary above)."
            else
                echo "Coverage generation failed."
            fi
        fi
        echo '```'
        echo

        # try to parse coverage-summary.json if present and jq is available
        if [ -f target/llvm-cov/coverage-summary.json ]; then
            echo "### Coverage by Crate"
            echo '```'
            if has_cmd jq; then
                jq -r '.packages[] | "\(.name): \(.coverage.line.percent)% lines"' target/llvm-cov/coverage-summary.json || true
            else
                # fallback: grep names and percentages
                grep -E '"name"|"line"' target/llvm-cov/coverage-summary.json | sed 'N;s/\n/ /' | sed -E 's/.*"name": ?"([^"]+)".*"percent": ?([0-9]+(\.[0-9]+)?)/\1: \2%/g' | uniq || true
            fi
            echo '```'
            echo
        fi
    else
        echo "cargo-llvm-cov not available - install or enable cargo llvm-cov"
        echo
    fi
} >> "$REPORT_FILE"

# --- Test Health Analysis ---------------------------------------------------
log "Scanning tests for common patterns and potential issues..."
{
    echo "## Test Health Analysis"
    echo

    echo "### Common Test Patterns"
    echo '```'
    # Search test files for assert patterns and common smells
    find . -type f -name '*.rs' \( -path './tests/*' -o -path './*/tests/*' -o -path './src/*' \) -print0 \
        | xargs -0 grep -h --line-number -E '(^|\b)assert(_eq|_ne|!)' 2>/dev/null | sort | uniq -c | sort -rn | head -20 || true
    echo
    echo '```'
    echo

    echo "### Potential Issues"
    echo '```'
    find . -type f -name '*.rs' \( -path './tests/*' -o -path './*/tests/*' \) -print0 \
        | xargs -0 grep -l -E 'sleep\(|thread::sleep|tokio::time::sleep' 2>/dev/null | head -10 || true
    echo
    find . -type f -name '*.rs' \( -path './tests/*' -o -path './*/tests/*' \) -print0 \
        | xargs -0 grep -l -E 'println!|eprintln!' 2>/dev/null | head -10 || true
    echo
    echo '```'
    echo
} >> "$REPORT_FILE" 2>/dev/null || true

# If previous append failed due to case, append correctly (defensive)
if ! grep -q "Test Health Analysis" "$REPORT_FILE" 2>/dev/null; then
    {
        echo "## Test Health Analysis"
        echo " (scan skipped due to errors)"
    } >> "$REPORT_FILE"
fi

# --- Recommendations --------------------------------------------------------
log "Generating recommendations..."
{
    echo "## Quality Recommendations"
    echo
    echo "### High Priority"
    if [ "$TOTAL_TESTS" -lt 500 ]; then
        echo "- **Increase test coverage**: Current test count ($TOTAL_TESTS) is below recommended threshold (500+)."
    else
        echo "- Test count looks healthy: $TOTAL_TESTS tests."
    fi
    if [ -f "$TMP_TIMING_LOG" ]; then
        if grep -E "(PASS|FAIL).*\[[0-9]+(\.[0-9]+)?s\]" "$TMP_TIMING_LOG" >/dev/null 2>&1; then
            if awk '/(PASS|FAIL)/ { if (match($0, /\[([0-9]+(\.[0-9]+)?)s\]/, m)) if (m[1]+0>10) {print; exit 0} }' "$TMP_TIMING_LOG"; then
                echo "- **Optimize slow tests**: some tests exceed 10s — consider splitting or mocking."
            fi
        fi
    fi
    echo
    echo "### Medium Priority"
    echo "- Standardize test helpers and share where useful."
    echo "- Add property tests and more integration coverage for critical paths."
    echo
    echo "### Low Priority"
    echo "- Add doctests for public APIs and consider benchmark suites."
    echo
} >> "$REPORT_FILE"

# --- Metadata & Footer ------------------------------------------------------
END_TIME=$(date +%s)
ELAPSED_SEC=$((END_TIME - START_TIME))
ELAPSED_HHMMSS=$(printf '%02d:%02d:%02d' $((ELAPSED_SEC/3600)) $((ELAPSED_SEC%3600/60)) $((ELAPSED_SEC%60)))

{
    echo "## Audit Metadata"
    echo
    echo "- **Audit Duration:** ${ELAPSED_HHMMSS} (started: $(date -d "@$START_TIME" 2>/dev/null || date -r "$START_TIME" 2>/dev/null || date))"
    echo "- **Workspace State:** $(git status --porcelain 2>/dev/null | wc -l | tr -d ' ') uncommitted changes"
    echo "- **Tool Versions:**"
    echo "  - cargo: $(has_cmd cargo && cargo --version || echo 'not available')"
    echo "  - nextest: $( [ -n \"$nextest_cmd\" ] && $nextest_cmd --version 2>/dev/null || echo 'not installed' )"
    echo "  - llvm-cov: $( $llvm_cov_available && (cargo llvm-cov --version 2>/dev/null || echo 'available') || echo 'not available' )"
    echo
    echo "---"
    echo "*Generated by test-audit.sh*"
} >> "$REPORT_FILE"

success "Test audit completed."
success "Report: $REPORT_FILE"

# --- Optional: open the report ----------------------------------------------
get_opener(){
    for p in open xdg-open wslview sensible-browser; do
        has_cmd "$p" && printf '%s' "$p" && return 0
    done
    return 1
}
if opener=$(get_opener); then
    read -r -p "Open audit report? (y/N): " ans || true
    case "$ans" in [Yy]*) "$opener" "$REPORT_FILE" >/dev/null 2>&1 || warning "Failed to open report";; esac
fi
