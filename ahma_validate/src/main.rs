use ahma_core::mcp_service::GuidanceConfig;
use ahma_core::schema_validation::MtdfValidator;
use anyhow::{Result, anyhow};
use clap::Parser;
use serde_json::from_str;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tracing::{error, info, instrument};

/// Ahma MCP Tool Configuration Validator
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "Validates tool configurations against the MTDF schema and checks for other inconsistencies."
)]
struct Cli {
    /// Path to the directory containing tool JSON configuration files, a comma-separated list of files, or blank to validate '.ahma/tools'.
    #[arg(default_value = ".ahma/tools")]
    validation_target: String,

    /// Path to the tool guidance JSON file.
    #[arg(long, global = true, default_value = ".ahma/tool_guidance.json")]
    guidance_file: PathBuf,

    /// Enable debug logging.
    #[arg(short, long, global = true)]
    debug: bool,
}

#[instrument]
fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.debug { "debug" } else { "info" };
    ahma_core::utils::logging::init_logging(log_level, false)?;

    if run_validation_mode(&cli)? {
        info!("All configurations are valid.");
        Ok(())
    } else {
        Err(anyhow!(
            "Some configurations are invalid. Please check the error messages above."
        ))
    }
}

fn run_validation_mode(cli: &Cli) -> Result<bool> {
    let mut all_valid = true;

    // Load guidance configuration
    let guidance_content = fs::read_to_string(&cli.guidance_file)?;
    let _guidance_config: GuidanceConfig = from_str(&guidance_content)?;
    let validator = MtdfValidator::new();

    // Process each target in the validation target list
    let targets = if cli.validation_target.contains(',') {
        cli.validation_target
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    } else {
        vec![cli.validation_target.clone()]
    };

    let mut files_to_validate = Vec::new();
    for target in targets {
        let path = PathBuf::from(target);
        if path.is_dir() {
            files_to_validate.extend(get_json_files(&path)?);
        } else if path.is_file() {
            files_to_validate.push(path);
        } else {
            error!("Validation target not found: {}", path.display());
            all_valid = false;
        }
    }

    for file_path in files_to_validate {
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to read file {}: {}",
                    file_path.display(),
                    e.to_string()
                );
                all_valid = false;
                continue;
            }
        };

        match validator.validate_tool_config(&file_path, &content) {
            Ok(_) => {
                info!("{} is valid.", file_path.display());
            }
            Err(e) => {
                error!("Validation failed for {}: {:?}", file_path.display(), e);
                all_valid = false;
            }
        }
    }

    Ok(all_valid)
}

fn get_json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            files.push(path);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Helper to create a temp directory with optional files
    fn setup_temp_dir_with_files(files: &[(&str, &str)]) -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        for (name, content) in files {
            let file_path = temp_dir.path().join(name);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).expect("Failed to create parent dirs");
            }
            let mut file = fs::File::create(&file_path).expect("Failed to create file");
            file.write_all(content.as_bytes())
                .expect("Failed to write file");
        }
        temp_dir
    }

    // ==================== get_json_files tests ====================

    #[test]
    fn test_get_json_files_returns_only_json_files() {
        let temp_dir = setup_temp_dir_with_files(&[
            ("tool1.json", "{}"),
            ("tool2.json", "{}"),
            ("readme.txt", "text"),
            ("config.yaml", "yaml: true"),
        ]);

        let files = get_json_files(temp_dir.path()).expect("Should succeed");

        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|p| p.extension().unwrap() == "json"));
    }

    #[test]
    fn test_get_json_files_empty_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let files = get_json_files(temp_dir.path()).expect("Should succeed");

        assert!(files.is_empty());
    }

    #[test]
    fn test_get_json_files_no_json_files() {
        let temp_dir =
            setup_temp_dir_with_files(&[("readme.md", "# Readme"), ("config.toml", "[config]")]);

        let files = get_json_files(temp_dir.path()).expect("Should succeed");

        assert!(files.is_empty());
    }

    #[test]
    fn test_get_json_files_nonexistent_directory() {
        let result = get_json_files(Path::new("/nonexistent/path/12345"));

        assert!(result.is_err());
    }

    #[test]
    fn test_get_json_files_ignores_subdirectories() {
        let temp_dir =
            setup_temp_dir_with_files(&[("tool.json", "{}"), ("subdir/nested.json", "{}")]);

        let files = get_json_files(temp_dir.path()).expect("Should succeed");

        // Should only find top-level json files, not nested ones
        assert_eq!(files.len(), 1);
        assert!(files[0].file_name().unwrap() == "tool.json");
    }

    // ==================== CLI parsing tests ====================

    #[test]
    fn test_cli_default_values() {
        let cli = Cli::parse_from(["ahma_validate"]);

        assert_eq!(cli.validation_target, ".ahma/tools");
        assert_eq!(cli.guidance_file, PathBuf::from(".ahma/tool_guidance.json"));
        assert!(!cli.debug);
    }

    #[test]
    fn test_cli_custom_validation_target() {
        let cli = Cli::parse_from(["ahma_validate", "custom/path"]);

        assert_eq!(cli.validation_target, "custom/path");
    }

    #[test]
    fn test_cli_debug_flag() {
        let cli = Cli::parse_from(["ahma_validate", "--debug"]);

        assert!(cli.debug);
    }

    #[test]
    fn test_cli_short_debug_flag() {
        let cli = Cli::parse_from(["ahma_validate", "-d"]);

        assert!(cli.debug);
    }

    #[test]
    fn test_cli_custom_guidance_file() {
        let cli = Cli::parse_from(["ahma_validate", "--guidance-file", "custom_guidance.json"]);

        assert_eq!(cli.guidance_file, PathBuf::from("custom_guidance.json"));
    }

    #[test]
    fn test_cli_comma_separated_targets() {
        let cli = Cli::parse_from(["ahma_validate", "file1.json,file2.json,dir/"]);

        assert_eq!(cli.validation_target, "file1.json,file2.json,dir/");
    }

    // ==================== run_validation_mode tests ====================

    /// Creates a minimal valid MTDF tool configuration
    /// Required fields: name, description, command
    fn valid_tool_config() -> &'static str {
        r#"{
            "name": "test_tool",
            "description": "A test tool for validation",
            "command": "echo"
        }"#
    }

    /// Creates a minimal valid guidance configuration
    /// GuidanceConfig requires guidance_blocks field
    fn valid_guidance_config() -> &'static str {
        r#"{
            "guidance_blocks": {}
        }"#
    }

    #[test]
    fn test_run_validation_mode_valid_single_file() {
        let temp_dir = setup_temp_dir_with_files(&[
            ("tool.json", valid_tool_config()),
            ("tool_guidance.json", valid_guidance_config()),
        ]);

        let cli = Cli {
            validation_target: temp_dir
                .path()
                .join("tool.json")
                .to_string_lossy()
                .to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_run_validation_mode_valid_directory() {
        let temp_dir = setup_temp_dir_with_files(&[
            ("tools/tool1.json", valid_tool_config()),
            ("tools/tool2.json", valid_tool_config()),
            ("tool_guidance.json", valid_guidance_config()),
        ]);

        let cli = Cli {
            validation_target: temp_dir.path().join("tools").to_string_lossy().to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_run_validation_mode_comma_separated_files() {
        let temp_dir = setup_temp_dir_with_files(&[
            ("tool1.json", valid_tool_config()),
            ("tool2.json", valid_tool_config()),
            ("tool_guidance.json", valid_guidance_config()),
        ]);

        let file1 = temp_dir
            .path()
            .join("tool1.json")
            .to_string_lossy()
            .to_string();
        let file2 = temp_dir
            .path()
            .join("tool2.json")
            .to_string_lossy()
            .to_string();

        let cli = Cli {
            validation_target: format!("{},{}", file1, file2),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_run_validation_mode_nonexistent_target() {
        let temp_dir =
            setup_temp_dir_with_files(&[("tool_guidance.json", valid_guidance_config())]);

        let cli = Cli {
            validation_target: "/nonexistent/path/12345".to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false for invalid
    }

    #[test]
    fn test_run_validation_mode_invalid_json_content() {
        let temp_dir = setup_temp_dir_with_files(&[
            ("tool.json", "{ invalid json }"),
            ("tool_guidance.json", valid_guidance_config()),
        ]);

        let cli = Cli {
            validation_target: temp_dir
                .path()
                .join("tool.json")
                .to_string_lossy()
                .to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false for invalid JSON
    }

    #[test]
    fn test_run_validation_mode_missing_guidance_file() {
        let temp_dir = setup_temp_dir_with_files(&[("tool.json", valid_tool_config())]);

        let cli = Cli {
            validation_target: temp_dir
                .path()
                .join("tool.json")
                .to_string_lossy()
                .to_string(),
            guidance_file: PathBuf::from("/nonexistent/guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_err());
    }

    #[test]
    fn test_run_validation_mode_invalid_guidance_json() {
        let temp_dir = setup_temp_dir_with_files(&[
            ("tool.json", valid_tool_config()),
            ("tool_guidance.json", "{ not valid json }"),
        ]);

        let cli = Cli {
            validation_target: temp_dir
                .path()
                .join("tool.json")
                .to_string_lossy()
                .to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_err());
    }

    #[test]
    fn test_run_validation_mode_empty_directory() {
        let temp_dir =
            setup_temp_dir_with_files(&[("tool_guidance.json", valid_guidance_config())]);

        // Create an empty tools directory
        fs::create_dir(temp_dir.path().join("tools")).expect("Failed to create tools dir");

        let cli = Cli {
            validation_target: temp_dir.path().join("tools").to_string_lossy().to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(result.unwrap()); // Empty directory is valid (no files to fail)
    }

    #[test]
    fn test_run_validation_mode_mixed_valid_invalid() {
        let temp_dir = setup_temp_dir_with_files(&[
            ("tools/valid.json", valid_tool_config()),
            ("tools/invalid.json", "{ not json }"),
            ("tool_guidance.json", valid_guidance_config()),
        ]);

        let cli = Cli {
            validation_target: temp_dir.path().join("tools").to_string_lossy().to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(!result.unwrap()); // One invalid file should make whole result false
    }

    #[test]
    fn test_run_validation_mode_missing_required_fields() {
        // Tool config missing required 'name' field
        let invalid_tool = r#"{
            "description": "Missing name field",
            "inputSchema": {
                "type": "object"
            }
        }"#;

        let temp_dir = setup_temp_dir_with_files(&[
            ("tool.json", invalid_tool),
            ("tool_guidance.json", valid_guidance_config()),
        ]);

        let cli = Cli {
            validation_target: temp_dir
                .path()
                .join("tool.json")
                .to_string_lossy()
                .to_string(),
            guidance_file: temp_dir.path().join("tool_guidance.json"),
            debug: false,
        };

        let result = run_validation_mode(&cli);

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Missing required field should fail validation
    }
}
