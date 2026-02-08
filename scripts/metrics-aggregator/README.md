# Metrics Aggregator

A code health metrics aggregator that analyzes source code using [rust-code-analysis-cli](https://github.com/mozilla/rust-code-analysis) and generates comprehensive health reports.

## Installation

Requires `rust-code-analysis-cli`:

```bash
cargo binstall rust-code-analysis-cli
# Or from source:
cargo install --git https://github.com/mozilla/rust-code-analysis rust-code-analysis-cli
```

## Usage

```bash
# Analyze a single crate
cargo run -- /path/to/crate

# Analyze with HTML report
cargo run -- /path/to/project --html

# Custom output directory
cargo run -- /path/to/project -o my_results

# Limit emergency items shown
cargo run -- /path/to/project --limit 5

# Open report automatically
cargo run -- /path/to/project --html --open

# Analyze multiple languages (comma-separated list)
cargo run -- /path/to/project --extensions rs,py,js

# All supported languages example
cargo run -- /path/to/project --extensions rs,py,js,ts,tsx,c,h,cpp,cc,hpp,hh,cs,java,go,css,html
```

## Scoring Formula

Each file receives a health score (0-100%) based on:

```
Score = 0.6 × MI + 0.2 × Cog_Score + 0.2 × Cyc_Score
```

Where:
- **MI**: Maintainability Index (Visual Studio variant, 0-100)
- **Cog_Score**: `(100 - (cognitive / sloc * 100)).max(0.0)`
- **Cyc_Score**: `(100 - (cyclomatic / sloc * 100)).max(0.0)`

### Weighting Rationale

| Metric | Weight | Rationale |
|--------|--------|-----------|
| Maintainability Index | 60% | Composite metric already balancing complexity, volume, and LOC |
| Cognitive Complexity | 20% | Measures comprehension difficulty per 100 lines |
| Cyclomatic Complexity | 20% | Measures testability per 100 lines |

### Normalization

Complexity metrics are normalized by SLOC to calculate density per 100 lines. This ensures small dense files are penalized more than large sparse files with the same total complexity.

### Known Limitations

1. **Double-counting**: MI already incorporates cyclomatic complexity in its formula, so cyclomatic is partially double-weighted.

2. **Clamping**: Density >100 per 100 lines is treated the same as density=100 for scoring purposes.

## Output

### Markdown Report (`CODE_HEALTH_REPORT.md`)

- Overall repository health percentage
- Per-crate/package health breakdown
- Top N "code health emergencies" with culprit identification
- Metrics glossary

### HTML Report (`CODE_HEALTH_REPORT.html`)

Styled HTML version of the markdown report (use `--html` flag).

## Metrics Explained

| Metric | Description | Good | Concerning |
|--------|-------------|------|------------|
| Cognitive Complexity | How hard to understand control flow | <10 | >20 |
| Cyclomatic Complexity | Number of independent paths | <10 | >20 |
| SLOC | Source lines of code | <300 | >500 |
| Maintainability Index | Ease of maintenance (higher=better) | >50 | <30 |

## Workspace vs Single Crate

- **Workspace detected**: Reports "Health by Crate" with per-member breakdown
- **Single crate**: Reports "Health by Package" with directory-based grouping

## Multi-Language Support

Analyzes multiple languages using a comma-separated list of extensions via the `--extensions` (or `-e`) flag.

**Supported languages:**
- **Rust**: `rs`
- **Python**: `py`
- **JavaScript**: `js`
- **TypeScript**: `ts`, `tsx`
- **C**: `c`, `h`
- **C++**: `cpp`, `cc`, `hpp`, `hh`
- **C#**: `cs`
- **Java**: `java`
- **Go**: `go`
- **CSS**: `css`
- **HTML**: `html`

Example usage:
```bash
cargo run -- . --extensions rs,py,js
```

## Development

```bash
# Run tests
cargo test

# Build
cargo build --release
```
