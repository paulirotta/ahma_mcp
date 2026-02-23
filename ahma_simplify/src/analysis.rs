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

fn func_space_to_space_entry(space: &FuncSpace) -> SpaceEntry {
    let kind_str = match space.kind {
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
    .to_string();

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
    if let Some(prefix) = pattern_segment.strip_suffix('*') {
        component.starts_with(prefix)
    } else {
        component == pattern_segment
    }
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

fn should_exclude(path: &Path, custom_excludes: &[String]) -> bool {
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

    let count = source_files(dir, &allowed_exts, custom_excludes)
        .try_fold(0usize, |count, path| {
            write_metrics_toml(&path, dir, output_dir)?;
            Ok(count + 1)
        })?;

    println!("  Analyzed {} files.", count);
    Ok(())
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
        .filter_map(move |entry| {
            let path = entry.into_path();
            let ext = path.extension()?.to_str()?.to_lowercase();
            if !allowed_exts.is_empty() && !allowed_exts.contains(ext.as_str()) {
                return None;
            }
            if should_exclude(&path, custom_excludes) {
                return None;
            }
            Some(path)
        })
}

/// Analyse a single file and write its metrics as TOML into `output_dir`.
fn write_metrics_toml(path: &Path, dir: &Path, output_dir: &Path) -> Result<()> {
    let Some(results) = analyze_file(path) else {
        return Ok(());
    };
    let toml_content =
        toml::to_string(&results).context("Failed to serialize metrics to TOML")?;
    let relative = path.strip_prefix(dir).unwrap_or(path);
    let toml_path = output_dir.join(relative.with_extension("toml"));
    if let Some(parent) = toml_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
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
    if !is_workspace {
        return run_analysis(directory, output, extensions, custom_excludes);
    }

    let members = get_workspace_members(directory)?;
    let member_dirs: Vec<PathBuf> = members
        .iter()
        .map(|m| directory.join(m))
        .filter(|p| p.is_dir())
        .collect();

    if member_dirs.is_empty() {
        return run_analysis(directory, output, extensions, custom_excludes);
    }

    for target_path in member_dirs {
        run_analysis(&target_path, output, extensions, custom_excludes)?;
    }
    Ok(())
}

/// Dynamically detect workspace members by:
/// 1. Parsing [workspace] members from Cargo.toml if present
/// 2. Falling back to directories containing Cargo.toml
fn get_workspace_members(dir: &Path) -> Result<Vec<String>> {
    if let Some(members) = parse_explicit_workspace_members(dir) {
        return Ok(members);
    }
    Ok(discover_member_directories(dir))
}

/// Parse explicit workspace members from `[workspace] members` in Cargo.toml.
fn parse_explicit_workspace_members(dir: &Path) -> Option<Vec<String>> {
    let content = fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let value: toml::Value = content.parse().ok()?;
    let members_array = value
        .get("workspace")?
        .get("members")?
        .as_array()?;
    let explicit: Vec<String> = members_array
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.to_string())
        .collect();
    if explicit.is_empty() { None } else { Some(explicit) }
}

/// Fallback: discover subdirectories that contain a Cargo.toml.
fn discover_member_directories(dir: &Path) -> Vec<String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            let dominated = !path.is_dir()
                || !path.join("Cargo.toml").exists()
                || name == "target"
                || name == ".git"
                || name.starts_with('.');
            if dominated { None } else { Some(name.to_string()) }
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
    let relative = get_relative_path(path, base_dir);
    relative
        .components()
        .find_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn is_cargo_workspace(dir: &Path) -> bool {
    let cargo_toml = dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        return false;
    }

    fs::read_to_string(cargo_toml)
        .map(|content| content.contains("[workspace]"))
        .unwrap_or(false)
}
