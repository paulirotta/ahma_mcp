# ahma_code_simplicity — Requirements

## Purpose

`ahma_code_simplicity` is a code simplicity metrics aggregator that uses [rust-code-analysis-cli](https://github.com/mozilla/rust-code-analysis) to analyze source code and generate comprehensive simplicity reports (Markdown and HTML).

It is a workspace member of the [Ahma MCP](../REQUIREMENTS.md) project.

## Functional Requirements

### R1: External Dependency

- **R1.1**: The tool requires `rust-code-analysis-cli` to be installed and available on `$PATH`.
- **R1.2**: On missing dependency, the tool must exit with a clear installation instruction.

### R2: Analysis Scope

- **R2.1**: Accepts a directory path as the primary argument.
- **R2.2**: Detects Cargo workspaces and analyzes each member crate individually.
- **R2.3**: Falls back to single-directory analysis when no workspace is detected.
- **R2.4**: Supports multi-language analysis via `--extensions` flag (default: all supported extensions).
- **R2.5**: Applies default exclusion patterns (target, node_modules, build dirs, VCS, IDE configs).
- **R2.6**: Supports additional user-defined exclusion patterns via `--exclude`.

### R3: Simplicity Scoring

- **R3.1**: Each file receives a composite simplicity score (0–100%) using:
  ```
  Score = 0.6 × MI + 0.2 × Cog_Score + 0.2 × Cyc_Score
  ```
- **R3.2**: Cognitive and cyclomatic complexity are SLOC-normalized (density per 100 lines).
- **R3.3**: Trivial/empty files (MI=0, cognitive=0, cyclomatic≤1) receive a perfect 100% score.

### R4: Report Generation

- **R4.1**: Always generates `CODE_SIMPLICITY.md` in the analyzed directory.
- **R4.2**: `--html` flag additionally generates `CODE_SIMPLICITY.html` with styled output.
- **R4.3**: Reports include: overall simplicity, per-language breakdown, per-crate/package scores, top N issues, and a metrics glossary.
- **R4.4**: `--limit N` controls how many issues are listed (default: 10).
- **R4.5**: `--open` flag opens the report in the default system viewer.

### R5: Output Directory

- **R5.1**: Intermediate `rust-code-analysis-cli` output is stored in a configurable directory (`--output`, default: `analysis_results`).
- **R5.2**: The output directory is cleared before each run.

## Non-Functional Requirements

- **NF1**: The tool is a synchronous CLI binary (no async runtime needed).
- **NF2**: Uses workspace-level dependency versions where available.
- **NF3**: All tests must be deterministic and fast (<100ms each).

## CI Integration

The `job-coverage` step in `.github/workflows/build.yml` runs `ahma_code_simplicity` and publishes `CODE_SIMPLICITY.md` / `CODE_SIMPLICITY.html` to GitHub Pages alongside the coverage report.
