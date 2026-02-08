use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

use super::types::SandboxMode;

/// Canonicalize and validate a list of sandbox scopes.
///
/// Rejects root filesystem (`/`) and empty paths in Strict mode.
/// Falls back to raw paths in Test mode when canonicalization fails.
pub(super) fn canonicalize_scopes(
    scopes: Vec<PathBuf>,
    mode: SandboxMode,
    context: &str,
) -> Result<Vec<PathBuf>> {
    let mut canonicalized = Vec::with_capacity(scopes.len());
    for scope in scopes {
        if mode != SandboxMode::Test && (scope == Path::new("/") || scope == Path::new("")) {
            return Err(anyhow!(
                "Root '/' or empty path is not a valid sandbox scope. {}",
                context
            ));
        }

        let canonical = match std::fs::canonicalize(&scope) {
            Ok(c) => c,
            Err(e) => {
                if mode == SandboxMode::Test {
                    scope.clone()
                } else {
                    return Err(anyhow!(
                        "Failed to canonicalize sandbox scope '{}': {}",
                        scope.display(),
                        e
                    ));
                }
            }
        };

        if mode != SandboxMode::Test && canonical == Path::new("/") {
            return Err(anyhow!(
                "Root '/' is not a valid sandbox scope (resolved from '{}'). {}",
                scope.display(),
                context
            ));
        }

        canonicalized.push(canonical);
    }
    Ok(canonicalized)
}

/// Normalize a path lexically (without filesystem access).
pub fn normalize_path_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut stack = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.last().is_some_and(|c| *c != Component::RootDir) {
                    stack.pop();
                }
            }
            c => stack.push(c),
        }
    }

    stack.iter().collect()
}
