use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(author, version, about = "Analyzes Rust code metrics and generates a health report", long_about = None)]
struct Cli {
    /// Directory to analyze (absolute or relative)
    directory: PathBuf,

    /// Output directory for analysis results
    #[arg(short, long, default_value = "analysis_results")]
    output: PathBuf,
}

#[derive(Debug, Deserialize)]
struct MetricsResults {
    name: String,
    metrics: Metrics,
}

#[derive(Debug, Deserialize)]
struct Metrics {
    cognitive: Cognitive,
    cyclomatic: Cyclomatic,
    mi: Mi,
    loc: Loc,
}

#[derive(Debug, Deserialize)]
struct Cognitive {
    sum: f64,
}

#[derive(Debug, Deserialize)]
struct Cyclomatic {
    sum: f64,
}

#[derive(Debug, Deserialize)]
struct Mi {
    mi_visual_studio: f64,
}

#[derive(Debug, Deserialize)]
struct Loc {
    sloc: f64,
}

#[derive(Debug)]
struct FileHealth {
    path: String,
    score: f64,
    cognitive: f64,
    cyclomatic: f64,
    sloc: f64,
    mi: f64,
}

impl FileHealth {
    fn calculate(results: &MetricsResults) -> Self {
        let mi = results.metrics.mi.mi_visual_studio;
        let cognitive = results.metrics.cognitive.sum;
        let cyclomatic = results.metrics.cyclomatic.sum;
        let sloc = results.metrics.loc.sloc;

        // HealthScore = max(0, MI_visual_studio - (Cognitive * 0.5) - (Cyclomatic * 0.2) - (SLOC / 500))
        let score =
            (mi - (cognitive * 0.5) - (cyclomatic * 0.2) - (sloc / 500.0)).clamp(0.0, 100.0);

        Self {
            path: results.name.clone(),
            score,
            cognitive,
            cyclomatic,
            sloc,
            mi,
        }
    }
}

fn check_dependencies() -> Result<()> {
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

fn run_analysis(dir: &Path, output_dir: &Path) -> Result<()> {
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    check_dependencies()?;

    if !cli.directory.exists() {
        anyhow::bail!("Directory does not exist: {}", cli.directory.display());
    }

    // Create output directory
    fs::create_dir_all(&cli.output).context("Failed to create output directory")?;

    // In a real workspace, we might want to find all members.
    // For now, we follow the logic of analyzing the provided directory.
    // However, if it's the root, we might want to analyze subdirectories like the script did.
    // Let's check for subdirectories with Cargo.toml or just analyze the whole thing if it's small.
    // The script hardcoded: ahma_common ahma_core ahma_http_bridge ahma_http_mcp_client ahma_mcp ahma_validate

    // To match the script's behavior exactly when run on root:
    let targets = vec![
        "ahma_common",
        "ahma_core",
        "ahma_http_bridge",
        "ahma_http_mcp_client",
        "ahma_mcp",
        "ahma_validate",
    ];

    let mut analyzed_something = false;
    for target in targets {
        let target_path = cli.directory.join(target);
        if target_path.is_dir() {
            run_analysis(&target_path, &cli.output)?;
            analyzed_something = true;
        }
    }

    if !analyzed_something {
        // Fallback: analyze the directory itself
        run_analysis(&cli.directory, &cli.output)?;
    }

    let mut files_health = Vec::new();
    println!("Aggregating metrics from {}...", cli.output.display());

    for entry in WalkDir::new(&cli.output).into_iter().filter_map(|e| e.ok()) {
        if entry.path().extension().is_some_and(|ext| ext == "toml") {
            let content = fs::read_to_string(entry.path())?;
            match toml::from_str::<MetricsResults>(&content) {
                Ok(results) => {
                    files_health.push(FileHealth::calculate(&results));
                }
                Err(e) => {
                    eprintln!("Error parsing {}: {}", entry.path().display(), e);
                }
            }
        }
    }

    if files_health.is_empty() {
        println!("No analysis files found in {}.", cli.output.display());
        return Ok(());
    }

    // Sort by score (worst first)
    files_health.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());

    generate_report(&files_health)?;

    println!("Report generated: CODE_HEALTH_REPORT.md");
    Ok(())
}

fn generate_report(files: &[FileHealth]) -> Result<(), std::io::Error> {
    let report = create_report(files);
    fs::write("CODE_HEALTH_REPORT.md", report)?;
    Ok(())
}

fn create_report(files: &[FileHealth]) -> String {
    let mut report = String::new();
    report.push_str("# Unified Code Health Report\n\n");

    let avg_score = files.iter().map(|f| f.score).sum::<f64>() / files.len() as f64;
    report.push_str(&format!(
        "## Overall Repository Health: **{:.1}%**\n\n",
        avg_score
    ));

    report.push_str("### Executive Summary\n");
    if avg_score > 80.0 {
        report.push_str("The repository is in good health overall. Focus on isolated high-complexity files.\n\n");
    } else if avg_score > 60.0 {
        report.push_str("The repository has moderate technical debt. Consider refactoring the top medical emergencies.\n\n");
    } else {
        report.push_str("The repository requires significant architectural review. Multiple areas show high risk.\n\n");
    }

    report.push_str("## Top 10 Medical Emergencies (Lowest Health Score)\n\n");
    report.push_str("| File | Health Score | Cognitive | Cyclomatic | SLOC | Primary Culprit |\n");
    report.push_str("| :--- | :--- | :--- | :--- | :--- | :--- |\n");

    for f in files.iter().take(10) {
        let culprit = if f.cognitive > 20.0 {
            "High Cognitive Complexity"
        } else if f.cyclomatic > 20.0 {
            "High Cyclomatic Complexity"
        } else if f.sloc > 500.0 {
            "Mega-file"
        } else if f.mi < 50.0 {
            "Low Maintainability Index"
        } else {
            "General Complexity"
        };

        report.push_str(&format!(
            "| {} | **{:.1}** | {} | {} | {} | {} |\n",
            f.path, f.score, f.cognitive, f.cyclomatic, f.sloc, culprit
        ));
    }

    report.push_str("\n## Health by Package\n\n");
    let mut package_scores: HashMap<String, Vec<f64>> = HashMap::new();
    for f in files {
        let package = f.path.split('/').next().unwrap_or("unknown").to_string();
        package_scores.entry(package).or_default().push(f.score);
    }

    let mut package_avg: Vec<(String, f64)> = package_scores
        .into_iter()
        .map(|(p, scores)| {
            let avg = scores.iter().sum::<f64>() / scores.len() as f64;
            (p, avg)
        })
        .collect();
    package_avg.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    report.push_str("| Package | Avg Health Score |\n");
    report.push_str("| :--- | :--- |\n");
    for (p, score) in package_avg {
        report.push_str(&format!("| {} | {:.1}% |\n", p, score));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let args = vec!["metrics-aggregator", ".", "--output", "results"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.directory, PathBuf::from("."));
        assert_eq!(cli.output, PathBuf::from("results"));
    }

    #[test]
    fn test_file_health_calculate_perfect() {
        let results = MetricsResults {
            name: "perfect.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 0.0 },
                cyclomatic: Cyclomatic { sum: 0.0 },
                mi: Mi {
                    mi_visual_studio: 100.0,
                },
                loc: Loc { sloc: 0.0 },
            },
        };
        let health = FileHealth::calculate(&results);
        assert_eq!(health.score, 100.0);
        assert_eq!(health.path, "perfect.rs");
    }

    #[test]
    fn test_file_health_calculate_complex() {
        let results = MetricsResults {
            name: "complex.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 50.0 },   // -25
                cyclomatic: Cyclomatic { sum: 50.0 }, // -10
                mi: Mi {
                    mi_visual_studio: 60.0,
                }, // 60
                loc: Loc { sloc: 500.0 },             // -1
            },
        };
        // 60 - 25 - 10 - 1 = 24
        let health = FileHealth::calculate(&results);
        assert_eq!(health.score, 24.0);
    }

    #[test]
    fn test_file_health_calculate_clamped() {
        let results = MetricsResults {
            name: "awful.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 500.0 },
                cyclomatic: Cyclomatic { sum: 500.0 },
                mi: Mi {
                    mi_visual_studio: 0.0,
                },
                loc: Loc { sloc: 5000.0 },
            },
        };
        let health = FileHealth::calculate(&results);
        assert_eq!(health.score, 0.0);

        let great = MetricsResults {
            name: "great.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 0.0 },
                cyclomatic: Cyclomatic { sum: 0.0 },
                mi: Mi {
                    mi_visual_studio: 150.0,
                },
                loc: Loc { sloc: 0.0 },
            },
        };
        let health_great = FileHealth::calculate(&great);
        assert_eq!(health_great.score, 100.0);
    }

    #[test]
    fn test_create_report_structure() {
        let files = vec![
            FileHealth {
                path: "pkg1/file1.rs".to_string(),
                score: 80.0,
                cognitive: 10.0,
                cyclomatic: 5.0,
                sloc: 100.0,
                mi: 100.0,
            },
            FileHealth {
                path: "pkg2/file2.rs".to_string(),
                score: 40.0,
                cognitive: 30.0,
                cyclomatic: 25.0,
                sloc: 600.0,
                mi: 40.0,
            },
        ];

        let report = create_report(&files);
        assert!(report.contains("# Unified Code Health Report"));
        assert!(report.contains("## Overall Repository Health: **60.0%**"));
        assert!(report.contains("pkg1/file1.rs"));
        assert!(report.contains("pkg2/file2.rs"));
        assert!(report.contains("High Cognitive Complexity")); // for pkg2/file2.rs
        assert!(report.contains("| pkg1 | 80.0% |"));
        assert!(report.contains("| pkg2 | 40.0% |"));
    }

    #[test]
    fn test_create_report_executive_summary() {
        let files_good = vec![FileHealth {
            path: "f.rs".to_string(),
            score: 90.0,
            cognitive: 0.0,
            cyclomatic: 0.0,
            sloc: 0.0,
            mi: 90.0,
        }];
        assert!(create_report(&files_good).contains("good health overall"));

        let files_mid = vec![FileHealth {
            path: "f.rs".to_string(),
            score: 70.0,
            cognitive: 0.0,
            cyclomatic: 0.0,
            sloc: 0.0,
            mi: 70.0,
        }];
        assert!(create_report(&files_mid).contains("moderate technical debt"));

        let files_bad = vec![FileHealth {
            path: "f.rs".to_string(),
            score: 30.0,
            cognitive: 0.0,
            cyclomatic: 0.0,
            sloc: 0.0,
            mi: 30.0,
        }];
        assert!(create_report(&files_bad).contains("significant architectural review"));
    }

    #[test]
    fn test_create_report_package_grouping() {
        let files = vec![
            FileHealth {
                path: "pkg1/a.rs".to_string(),
                score: 100.0,
                cognitive: 0.0,
                cyclomatic: 0.0,
                sloc: 0.0,
                mi: 100.0,
            },
            FileHealth {
                path: "pkg1/b.rs".to_string(),
                score: 80.0,
                cognitive: 0.0,
                cyclomatic: 0.0,
                sloc: 0.0,
                mi: 80.0,
            },
            FileHealth {
                path: "root_file.rs".to_string(),
                score: 50.0,
                cognitive: 0.0,
                cyclomatic: 0.0,
                sloc: 0.0,
                mi: 50.0,
            },
        ];

        let report = create_report(&files);
        // pkg1 avg = (100+80)/2 = 90
        assert!(report.contains("| pkg1 | 90.0% |"));
        // root_file.rs package = root_file.rs (since split('/').next() returns the first part)
        assert!(report.contains("| root_file.rs | 50.0% |"));
    }
}
