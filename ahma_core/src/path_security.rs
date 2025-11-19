use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Validates that a path is within the specified root directory.
/// Resolves symlinks and relative paths.
pub fn validate_path(path: &Path, root: &Path) -> Result<PathBuf> {
    let root_canonical = fs::canonicalize(root)
        .map_err(|e| anyhow!("Failed to canonicalize root path {:?}: {}", root, e))?;

    // If path is absolute, check it directly. If relative, join with root.
    let path_to_check = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root_canonical.join(path)
    };

    // Try to canonicalize the full path to handle symlinks correctly
    let resolved_path = match fs::canonicalize(&path_to_check) {
        Ok(p) => p,
        Err(_) => {
            // If the file doesn't exist, we fall back to lexical normalization.
            // This is less secure against symlink attacks (e.g. if a component is a symlink to outside)
            // but necessary for creating new files.
            // A stricter approach would be to canonicalize the parent, but let's start here.
            normalize_path(&path_to_check)
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

/// Heuristically checks a shell command for suspicious paths.
pub fn validate_command(command: &str, root: &Path) -> Result<()> {
    let tokens = split_shell_command(command);

    for token in tokens {
        // Check if token looks like a path (contains / or is ..)
        if token.contains('/') || token == ".." {
            // Handle flags like --output=/tmp/foo
            let path_str = if token.starts_with('-') {
                if let Some((_, val)) = token.split_once('=') {
                    val
                } else {
                    continue; // Just a flag
                }
            } else {
                &token
            };

            if path_str.is_empty() {
                continue;
            }

            // If it still looks like a path after stripping flag
            if path_str.contains('/') || path_str == ".." {
                let path = Path::new(path_str);
                validate_path(path, root)
                    .map_err(|e| anyhow!("Command contains unsafe path '{}': {}", path_str, e))?;
            }
        }
    }
    Ok(())
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

fn split_shell_command(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    for c in command.chars() {
        if escaped {
            current_token.push(c);
            escaped = false;
            continue;
        }

        match c {
            '\\' => escaped = true,
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ' ' | '\t' | '\n' if !in_single_quote && !in_double_quote => {
                if !current_token.is_empty() {
                    tokens.push(current_token);
                    current_token = String::new();
                }
            }
            _ => current_token.push(c),
        }
    }
    if !current_token.is_empty() {
        tokens.push(current_token);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_validate_path_inside() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path();
        let file = root.join("foo.txt");
        fs::write(&file, "content")?;

        let validated = validate_path(&file, root)?;
        assert_eq!(validated, fs::canonicalize(&file)?);
        Ok(())
    }

    #[test]
    fn test_validate_path_outside() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path();
        let outside = root.join("../outside.txt");

        assert!(validate_path(&outside, root).is_err());
        Ok(())
    }

    #[test]
    fn test_validate_command_safe() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path();
        validate_command("ls -la .", root)?;
        validate_command("echo hello", root)?;
        Ok(())
    }

    #[test]
    fn test_validate_command_unsafe() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path();
        assert!(validate_command("ls /", root).is_err());
        assert!(validate_command("cat ../secret", root).is_err());
        Ok(())
    }
}
