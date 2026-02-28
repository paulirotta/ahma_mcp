use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::conversion::analyze_file;
use super::exclusion::should_exclude;
use super::workspace::workspace_analysis_dirs;

// ---------------------------------------------------------------------------
// Public analysis API (drop-in replacement for the old CLI-based version)
// ---------------------------------------------------------------------------

/// Analyses all source files under `dir` and writes per-file TOML metric
/// results into `output_dir`, mirroring the old `rust-code-analysis-cli`
/// output format so that `--verify` and the rest of main.rs continue to work.
pub fn run_analysis(
    dir: &Path,
    output_dir: &Path,
    extensions: &[String],
    custom_excludes: &[String],
) -> Result<()> {
    println!("Analyzing {}...", dir.display());

    let allowed_exts: std::collections::HashSet<&str> = extensions
        .iter()
        .map(|e| e.trim_start_matches('.'))
        .collect();

    let count = source_files(dir, &allowed_exts, custom_excludes).try_fold(
        0usize,
        |count, path| -> Result<usize> {
            write_metrics_toml(&path, dir, output_dir)?;
            Ok(count + 1)
        },
    )?;

    println!("  Analyzed {} files.", count);
    Ok(())
}

/// Check if a file matches the extension filter and is not excluded.
fn is_matching_source_file(
    path: &Path,
    allowed_exts: &std::collections::HashSet<&str>,
    custom_excludes: &[String],
) -> bool {
    if should_exclude(path, custom_excludes) {
        return false;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            allowed_exts.is_empty() || allowed_exts.contains(ext.to_lowercase().as_str())
        })
}

/// Iterate source files in `dir` matching extension and exclusion filters.
fn source_files<'a>(
    dir: &'a Path,
    allowed_exts: &'a std::collections::HashSet<&'a str>,
    custom_excludes: &'a [String],
) -> impl Iterator<Item = PathBuf> + 'a {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(move |path| is_matching_source_file(path, allowed_exts, custom_excludes))
}

/// Ensure the parent directory of `path` exists.
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    Ok(())
}

/// Analyse a single file and write its metrics as TOML into `output_dir`.
fn write_metrics_toml(path: &Path, dir: &Path, output_dir: &Path) -> Result<()> {
    let Some(results) = analyze_file(path) else {
        return Ok(());
    };
    let toml_content = toml::to_string(&results).context("Failed to serialize metrics to TOML")?;
    let relative = path.strip_prefix(dir).unwrap_or(path);
    let toml_path = output_dir.join(relative.with_extension("toml"));
    ensure_parent_dir(&toml_path)?;
    fs::write(&toml_path, toml_content)
        .with_context(|| format!("Failed to write {}", toml_path.display()))?;
    Ok(())
}

pub fn perform_analysis(
    directory: &Path,
    output: &Path,
    is_workspace: bool,
    extensions: &[String],
    custom_excludes: &[String],
) -> Result<()> {
    let dirs = workspace_analysis_dirs(directory, is_workspace)?;
    for dir in &dirs {
        run_analysis(dir, output, extensions, custom_excludes)?;
    }
    Ok(())
}
