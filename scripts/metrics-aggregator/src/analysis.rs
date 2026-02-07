use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn check_dependencies() -> Result<()> {
    let output = Command::new("rust-code-analysis-cli")
        .arg("--version")
        .output();

    if output.is_err() {
        anyhow::bail!(
            "rust-code-analysis-cli not found. Please install it using:\n\
             cargo binstall rust-code-analysis-cli\n\
             Or from source:\n\
             cargo install --git https://github.com/mozilla/rust-code-analysis rust-code-analysis-cli"
        );
    }
    Ok(())
}

pub fn run_analysis(dir: &Path, output_dir: &Path) -> Result<()> {
    println!("Analyzing {}...", dir.display());

    let status = Command::new("rust-code-analysis-cli")
        .arg("--paths")
        .arg(dir)
        .arg("--metrics")
        .arg("--function")
        .arg("--output-format")
        .arg("toml")
        .arg("--output")
        .arg(output_dir)
        .arg("--include")
        .arg("**/*.rs")
        .arg("--exclude")
        .arg("target/**")
        .status()
        .context("Failed to execute rust-code-analysis-cli")?;

    if !status.success() {
        anyhow::bail!("rust-code-analysis-cli failed for {}", dir.display());
    }

    Ok(())
}

pub fn perform_analysis(directory: &Path, output: &Path, is_workspace: bool) -> Result<()> {
    let mut analyzed_something = false;
    if is_workspace {
        let targets = vec![
            "ahma_common",
            "ahma_core",
            "ahma_http_bridge",
            "ahma_http_mcp_client",
            "ahma_mcp",
            "ahma_validate",
        ];

        for target in targets {
            let target_path = directory.join(target);
            if target_path.is_dir() {
                run_analysis(&target_path, output)?;
                analyzed_something = true;
            }
        }
    }

    if !analyzed_something {
        run_analysis(directory, output)?;
    }
    Ok(())
}

pub fn get_project_name(dir: &Path) -> String {
    let cargo_toml = dir.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(cargo_toml) {
        if let Ok(value) = content.parse::<toml::Value>() {
            if let Some(name) = value
                .get("package")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
            {
                return name.to_string();
            }
        }
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
