use anyhow::{Context, Result};
use rust_code_analysis::{
    FuncSpace, SpaceKind, get_function_spaces, get_language_for_file, read_file_with_eol,
};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::models::{Cognitive, Cyclomatic, Loc, Metrics, MetricsResults, Mi, SpaceEntry};

// ---------------------------------------------------------------------------
// Conversion helpers: rust-code-analysis native types → our MetricsResults
// ---------------------------------------------------------------------------

fn code_metrics_to_metrics(cm: &rust_code_analysis::CodeMetrics) -> Metrics {
    Metrics {
        cognitive: Cognitive {
            sum: cm.cognitive.cognitive_sum(),
        },
        cyclomatic: Cyclomatic {
            sum: cm.cyclomatic.cyclomatic_sum(),
        },
        mi: Mi {
            mi_visual_studio: cm.mi.mi_visual_studio(),
        },
        loc: Loc {
            sloc: cm.loc.sloc(),
        },
    }
}

fn space_kind_str(kind: SpaceKind) -> &'static str {
    match kind {
        SpaceKind::Function => "function",
        SpaceKind::Class => "class",
        SpaceKind::Struct => "struct",
        SpaceKind::Trait => "trait",
        SpaceKind::Impl => "impl",
        SpaceKind::Unit => "unit",
        SpaceKind::Namespace => "namespace",
        SpaceKind::Interface => "interface",
        SpaceKind::Unknown => "unknown",
    }
}

fn func_space_to_space_entry(space: &FuncSpace) -> SpaceEntry {
    let kind_str = space_kind_str(space.kind).to_string();

    SpaceEntry {
        name: space.name.clone().unwrap_or_default(),
        start_line: space.start_line as u32,
        end_line: space.end_line as u32,
        kind: kind_str,
        metrics: code_metrics_to_metrics(&space.metrics),
        spaces: space.spaces.iter().map(func_space_to_space_entry).collect(),
    }
}

fn func_space_to_metrics_results(space: FuncSpace) -> MetricsResults {
    MetricsResults {
        name: space.name.clone().unwrap_or_default(),
        metrics: code_metrics_to_metrics(&space.metrics),
        spaces: space.spaces.iter().map(func_space_to_space_entry).collect(),
    }
}

// ---------------------------------------------------------------------------
// Per-file analysis using the library
// ---------------------------------------------------------------------------

fn analyze_file(path: &Path) -> Option<MetricsResults> {
    let lang = get_language_for_file(path)?;
    let source = read_file_with_eol(path).ok().flatten()?;
    let func_space = get_function_spaces(&lang, source, path, None)?;
    Some(func_space_to_metrics_results(func_space))
}

// ---------------------------------------------------------------------------
// Exclusion filtering (replaces --exclude flags passed to the old CLI)
// ---------------------------------------------------------------------------

fn segment_matches(pattern_segment: &str, component: &str) -> bool {
    pattern_segment
        .strip_suffix('*')
        .map_or(component == pattern_segment, |prefix| {
            component.starts_with(prefix)
        })
}

/// Returns true if `path` should be excluded based on glob-style patterns.
/// Handles patterns of the form `**/<segment>/**` with optional trailing `*`
/// wildcard in the segment — covers every default exclusion and most
/// user-supplied ones.
fn pattern_matches_path(pattern: &str, path: &Path) -> bool {
    let segment = pattern.trim_start_matches("**/").trim_end_matches("/**");
    if segment.is_empty() {
        return false;
    }
    path.components()
        .any(|c| segment_matches(segment, &c.as_os_str().to_string_lossy()))
}

const DEFAULT_EXCLUDES: &[&str] = &[
    "**/target/**",
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
    "**/out/**",
    "**/bin/**",
    "**/obj/**",
    "**/venv/**",
    "**/.venv/**",
    "**/env/**",
    "**/.env/**",
    "**/__pycache__/**",
    "**/.tox/**",
    "**/.pytest_cache/**",
    "**/.mypy_cache/**",
    "**/.next/**",
    "**/.nuxt/**",
    "**/cmake-build-*/**",
    "**/analysis_results/**",
    "**/.git/**",
    "**/.svn/**",
    "**/.hg/**",
    "**/.idea/**",
    "**/.vscode/**",
];

fn should_exclude(path: &Path, custom_excludes: &[String]) -> bool {
    DEFAULT_EXCLUDES
        .iter()
        .any(|p| pattern_matches_path(p, path))
        || custom_excludes
            .iter()
            .any(|p| pattern_matches_path(p, path))
}

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

/// Resolve which directories to analyse: workspace members or just the root.
fn workspace_analysis_dirs(directory: &Path, is_workspace: bool) -> Result<Vec<PathBuf>> {
    if is_workspace {
        let dirs: Vec<_> = get_workspace_members(directory)?
            .into_iter()
            .map(|m| directory.join(m))
            .filter(|p| p.is_dir())
            .collect();
        if !dirs.is_empty() {
            return Ok(dirs);
        }
    }
    Ok(vec![directory.to_path_buf()])
}

/// Parse `[workspace] members` from a Cargo.toml string.
fn parse_workspace_members(content: &str) -> Option<Vec<String>> {
    let value: toml::Value = content.parse().ok()?;
    let members = value.get("workspace")?.get("members")?.as_array()?;
    let names: Vec<String> = members
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    (!names.is_empty()).then_some(names)
}

/// Detect workspace members from Cargo.toml `[workspace] members`, or fall
/// back to discovering subdirectories that contain a Cargo.toml.
fn get_workspace_members(dir: &Path) -> Result<Vec<String>> {
    let explicit = fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|c| parse_workspace_members(&c));
    Ok(explicit.unwrap_or_else(|| discover_member_directories(dir)))
}

/// Returns true if `path` looks like a Cargo workspace member directory.
fn is_workspace_member(path: &std::path::Path, name: &str) -> bool {
    path.is_dir() && path.join("Cargo.toml").exists() && name != "target" && !name.starts_with('.')
}

/// Fallback: discover subdirectories that contain a Cargo.toml.
fn discover_member_directories(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            is_workspace_member(&path, name).then(|| name.to_string())
        })
        .collect()
}

pub fn get_project_name(dir: &Path) -> String {
    let cargo_toml = dir.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(cargo_toml)
        && let Ok(value) = content.parse::<toml::Value>()
        && let Some(name) = value
            .get("package")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
    {
        return name.to_string();
    }
    dir.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn get_relative_path(path: &Path, base_dir: &Path) -> PathBuf {
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let abs_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());
    abs_path
        .strip_prefix(&abs_base)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| path.to_path_buf())
}

pub fn get_package_name(path: &Path, base_dir: &Path) -> String {
    get_relative_path(path, base_dir)
        .components()
        .find_map(|c| {
            if let std::path::Component::Normal(s) = c {
                Some(s.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn is_cargo_workspace(dir: &Path) -> bool {
    fs::read_to_string(dir.join("Cargo.toml"))
        .map(|content| content.contains("[workspace]"))
        .unwrap_or(false)
}
