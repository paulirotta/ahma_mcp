use std::path::Path;

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

pub(crate) fn should_exclude(path: &Path, custom_excludes: &[String]) -> bool {
    DEFAULT_EXCLUDES
        .iter()
        .any(|p| pattern_matches_path(p, path))
        || custom_excludes
            .iter()
            .any(|p| pattern_matches_path(p, path))
}
