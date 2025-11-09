use ahma_core::mcp_service::GuidanceConfig;
use ahma_core::schema_validation::MtdfValidator;
use anyhow::{anyhow, Result};
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
