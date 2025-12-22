use anyhow::{Result, anyhow};
use std::path::{Component, Path, PathBuf};
use tokio::fs;

/// Validates that a path is within the specified root directory.
/// Resolves symlinks and relative paths.
pub async fn validate_path(path: &Path, root: &Path) -> Result<PathBuf> {
    let root_canonical = fs::canonicalize(root)
        .await
        .map_err(|e| anyhow!("Failed to canonicalize root path {:?}: {}", root, e))?;

    // If path is absolute, check it directly. If relative, join with root.
    let path_to_check = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root_canonical.join(path)
    };

    // Try to canonicalize the full path to handle symlinks correctly.
    // If the file does not exist yet, canonicalize the parent directory (which should exist)
    // so symlink escapes are still detected for create/write operations.
    let resolved_path = match fs::canonicalize(&path_to_check).await {
        Ok(p) => p,
        Err(_) => {
            if let Some(parent) = path_to_check.parent() {
                if let Ok(parent_canonical) = fs::canonicalize(parent).await {
                    if let Some(name) = path_to_check.file_name() {
                        parent_canonical.join(name)
                    } else {
                        parent_canonical
                    }
                } else {
                    // If even the parent cannot be canonicalized, fall back to lexical normalization.
                    // This should be rare and is primarily for deeply-nested create flows.
                    normalize_path(&path_to_check)
                }
            } else {
                normalize_path(&path_to_check)
            }
        }
    };

    if resolved_path.starts_with(&root_canonical) {
        Ok(resolved_path)
    } else {
        Err(anyhow!(
            "Path {:?} is outside the sandbox root {:?}",
            path,
            root
        ))
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut stack = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                stack.pop();
            }
            Component::Normal(c) => stack.push(c),
            Component::RootDir => {
                stack.clear();
            }
            _ => {}
        }
    }

    let mut result = PathBuf::from("/");
    for c in stack {
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_validate_path_inside() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path();
        let file = root.join("foo.txt");
        fs::write(&file, "content").await?;

        let validated = validate_path(&file, root).await?;
        assert_eq!(validated, fs::canonicalize(&file).await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_validate_path_outside() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path();
        let outside = root.join("../outside.txt");

        assert!(validate_path(&outside, root).await.is_err());
        Ok(())
    }
}
