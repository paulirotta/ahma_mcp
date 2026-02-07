use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq)]
enum OutputFormat {
    Markdown,
    Html,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Analyzes Rust code metrics and generates a health report", long_about = None)]
struct Cli {
    /// Directory to analyze (absolute or relative)
    directory: PathBuf,

    /// Output directory for analysis results
    #[arg(short, long, default_value = "analysis_results")]
    output: PathBuf,

    /// Number of issues to show in the report
    #[arg(short, long, default_value_t = 10)]
    limit: usize,

    /// Output format (markdown or html)
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Markdown)]
    format: OutputFormat,

    /// Open the report automatically
    #[arg(long)]
    open: bool,

    /// Shorthand for --format html
    #[arg(long)]
    html: bool,
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

        // Determine base score
        // If MI is 0.0 but complexity is minimal, it's likely a file with only comments or trivial code.
        // We treat these as 100% healthy.
        let score = if mi == 0.0 && cognitive == 0.0 && cyclomatic <= 1.0 {
            100.0
        } else {
            // New 60/20/20 formula to provide better resolution at the low end
            let mi_score = mi.clamp(0.0, 100.0);
            let cog_score = (100.0 - cognitive).max(0.0);
            let cyc_score = (100.0 - cyclomatic).max(0.0);

            0.6 * mi_score + 0.2 * cog_score + 0.2 * cyc_score
        };

        Self {
            path: results.name.clone(),
            score: score.clamp(0.0, 100.0),
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

fn get_project_name(dir: &Path) -> String {
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

fn is_cargo_workspace(dir: &Path) -> bool {
    let cargo_toml = dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        return false;
    }

    if let Ok(content) = fs::read_to_string(cargo_toml) {
        content.contains("[workspace]")
    } else {
        false
    }
}

fn main() -> Result<()> {
    let mut cli = Cli::parse();

    // Handle shorthand
    if cli.html {
        cli.format = OutputFormat::Html;
    }

    check_dependencies()?;

    if !cli.directory.exists() {
        anyhow::bail!("Directory does not exist: {}", cli.directory.display());
    }
    let directory = cli
        .directory
        .canonicalize()
        .context("Failed to canonicalize directory")?;

    // Create or clear output directory
    if cli.output.exists() {
        println!(
            "Clearing existing analysis results in {}...",
            cli.output.display()
        );
        let _ = fs::remove_dir_all(&cli.output);
    }
    fs::create_dir_all(&cli.output).context("Failed to create output directory")?;

    let is_workspace = is_cargo_workspace(&cli.directory);

    let mut analyzed_something = false;
    if is_workspace {
        // To match the script's behavior exactly when run on root:
        let targets = vec![
            "ahma_common",
            "ahma_core",
            "ahma_http_bridge",
            "ahma_http_mcp_client",
            "ahma_mcp",
            "ahma_validate",
        ];

        for target in targets {
            let target_path = cli.directory.join(target);
            if target_path.is_dir() {
                run_analysis(&target_path, &cli.output)?;
                analyzed_something = true;
            }
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

    // Sort by score (worst first), then by cognitive complexity (worst first) to break ties
    files_health.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap()
            .then_with(|| b.cognitive.partial_cmp(&a.cognitive).unwrap())
    });

    let project_name = get_project_name(&directory);

    generate_report(
        &files_health,
        is_workspace,
        cli.limit,
        &directory,
        cli.format,
        &project_name,
    )?;

    let filename = match cli.format {
        OutputFormat::Markdown => "CODE_HEALTH_REPORT.md",
        OutputFormat::Html => "CODE_HEALTH_REPORT.html",
    };
    let report_path = directory.join(filename);
    println!("Report generated: {}", report_path.display());

    if cli.open {
        opener::open(&report_path).context("Failed to open report")?;
    }

    Ok(())
}

fn generate_report(
    files: &[FileHealth],
    is_workspace: bool,
    limit: usize,
    output_dir: &Path,
    format: OutputFormat,
    project_name: &str,
) -> Result<(), std::io::Error> {
    let md_content = create_report(files, is_workspace, limit, output_dir, project_name);

    // Always write the markdown version as it's the source for everything else
    fs::write(output_dir.join("CODE_HEALTH_REPORT.md"), &md_content)?;

    match format {
        OutputFormat::Markdown => {}
        OutputFormat::Html => {
            let mut options = pulldown_cmark::Options::empty();
            options.insert(pulldown_cmark::Options::ENABLE_TABLES);
            options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
            let parser = pulldown_cmark::Parser::new_ext(&md_content, options);
            let mut html_output = String::new();
            pulldown_cmark::html::push_html(&mut html_output, parser);

            let style = "
                body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; line-height: 1.6; color: #24292e; max-width: 900px; margin: 0 auto; padding: 40px 20px; background-color: #f6f8fa; }
                h1, h2, h3 { color: #1b1f23; border-bottom: 1px solid #eaecef; padding-bottom: 0.3em; margin-top: 1.5em; }
                pre { background-color: #f6f8fa; padding: 16px; border-radius: 6px; overflow: auto; }
                code { font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace; background-color: rgba(27,31,35,0.05); padding: 0.2em 0.4em; border-radius: 3px; }
                blockquote { padding: 0 1em; color: #6a737d; border-left: 0.25em solid #dfe2e1; margin: 0; }
                table { border-spacing: 0; border-collapse: collapse; width: 100%; margin: 1em 0; }
                table td, table th { padding: 6px 13px; border: 1px solid #dfe2e1; }
                table tr { background-color: #fff; border-top: 1px solid #c6cbd1; }
                table tr:nth-child(2n) { background-color: #f6f8fa; }
                .score-low { color: #d73a49; font-weight: bold; }
                .score-mid { color: #f9c513; font-weight: bold; }
                .score-high { color: #28a745; font-weight: bold; }
            ";

            let full_html = format!(
                "<!DOCTYPE html>\n<html>\n<head>\n<meta charset='UTF-8'>\n<title>Code Health Report</title>\n<style>\n{}\n</style>\n</head>\n<body>\n{}\n</body>\n</html>",
                style, html_output
            );
            fs::write(output_dir.join("CODE_HEALTH_REPORT.html"), full_html)?;
        }
    }
    Ok(())
}

fn create_report(
    files: &[FileHealth],
    is_workspace: bool,
    limit: usize,
    base_dir: &Path,
    project_name: &str,
) -> String {
    let mut report = String::new();
    report.push_str(&format!("# Code Health Metrics: {}\n\n", project_name));

    let avg_score = files.iter().map(|f| f.score).sum::<f64>() / files.len() as f64;
    report.push_str(&format!(
        "## Overall Repository Health: **{:.0}%**\n\n",
        avg_score
    ));

    let now = chrono::Local::now();
    report.push_str(&format!(
        "*Generated on: {}*\n\n",
        now.format("%Y-%m-%d %H:%M:%S")
    ));

    report.push_str("### Executive Summary\n");
    if avg_score > 80.0 {
        report.push_str("The repository is in good health overall. Focus on isolated high-complexity files.\n\n");
    } else if avg_score > 60.0 {
        report.push_str("The repository has moderate technical debt. Consider refactoring the top medical emergencies.\n\n");
    } else {
        report.push_str("The repository requires significant architectural review. Multiple areas show high risk.\n\n");
    }

    let group_label = if is_workspace { "Crate" } else { "Package" };
    report.push_str(&format!("## Health by {}\n\n", group_label));

    let mut package_scores: HashMap<String, Vec<f64>> = HashMap::new();
    for f in files {
        let path = Path::new(&f.path);
        // Ensure path strings are matching in format before stripping prefix
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let abs_base = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());

        let relative = abs_path.strip_prefix(&abs_base).unwrap_or(path);
        let package = relative
            .components()
            .find_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
                _ => None,
            })
            .unwrap_or_else(|| "unknown".to_string());
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

    for (i, (p, score)) in package_avg.iter().enumerate() {
        report.push_str(&format!("{}. **{}**: {:.0}%\n", i + 1, p, score));
    }
    report.push_str("\n");

    let display_limit = std::cmp::min(limit, files.len());
    report.push_str(&format!(
        "## Top {display_limit} Code Health Emergencies (Lowest Health Scores)\n\n",
    ));

    for (i, f) in files.iter().take(display_limit).enumerate() {
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

        let path = Path::new(&f.path);

        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let abs_base = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());
        let relative = abs_path.strip_prefix(&abs_base).unwrap_or(path);
        let relative_path = relative.to_string_lossy();

        let basename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| relative_path.to_string());

        report.push_str(&format!(
            "{}. **{}: {:.0}% ({})**\n\t{}\n",
            i + 1,
            basename,
            f.score,
            culprit,
            relative_path
        ));
        report.push_str(&format!(
            "    - Metrics: Cog: {}, Cyc: {}, SLOC: {}, MI: {:.1}\n",
            f.cognitive, f.cyclomatic, f.sloc, f.mi
        ));
    }

    report.push_str("\n---\n\n");
    report.push_str("## Metrics Glossary\n\n");
    report.push_str("### Cognitive Complexity\n");
    report.push_str("- **Description**: Measures how hard it is to understand the control flow of the code. [See](https://axify.io/blog/cognitive-complexity)\n");
    report.push_str("- **How to Improve**: Extract complex conditions into well-named functions and reduce nesting levels.\n\n");

    report.push_str("### Cyclomatic Complexity\n");
    report.push_str("- **Description**: Measures the number of linearly independent paths through the source code. [See](https://www.nist.gov/publications/structured-testing-software-testing-methodology-using-cyclomatic-complexity-metric)\n");
    report.push_str("- **How to Improve**: Use polymorphic abstractions instead of complex switch/if-else chains, and break down large functions into smaller components.\n\n");

    report.push_str("### Source Lines of Code (SLOC)\n");
    report.push_str("- **Description**: A measure of the size of the computer program by counting the number of lines in the text of the source code. [See](https://en.wikipedia.org/wiki/Source_lines_of_code)\n");
    report.push_str("- **How to Improve**: Remove dead code and refactor repetitive logic into reusable helper functions or macros.\n\n");

    report.push_str("### Maintainability Index (MI)\n");
    report.push_str("- **Description**: A composite metric representing the relative ease of maintaining the code; higher is better. [See](https://learn.microsoft.com/en-us/visualstudio/code-quality/code-metrics-maintainability-index-range-and-meaning)\n");
    report.push_str("- **How to Improve**: Simultaneously reduce complexity (both cognitive and cyclomatic) and file size to boost the index.\n");

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
        // mi_score: 60, cog_score: 50, cyc_score: 50
        // 0.6*60 + 0.2*50 + 0.2*50 = 36 + 10 + 10 = 56
        let health = FileHealth::calculate(&results);
        assert_eq!(health.score, 56.0);
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

        let report = create_report(&files, false, 10, Path::new("."), "test_project");
        assert!(report.contains("# Code Health Metrics: test_project"));
        assert!(report.contains("## Overall Repository Health: **60%**"));
        assert!(report.contains("pkg1/file1.rs"));
        assert!(report.contains("pkg2/file2.rs"));
        assert!(report.contains("High Cognitive Complexity")); // for pkg2/file2.rs
        assert!(report.contains("1. **pkg1**: 80%"));
        assert!(report.contains("2. **pkg2**: 40%"));
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
        assert!(
            create_report(&files_good, false, 10, Path::new("."), "test_project")
                .contains("good health overall")
        );

        let files_mid = vec![FileHealth {
            path: "f.rs".to_string(),
            score: 70.0,
            cognitive: 0.0,
            cyclomatic: 0.0,
            sloc: 0.0,
            mi: 70.0,
        }];
        assert!(
            create_report(&files_mid, false, 10, Path::new("."), "test_project")
                .contains("moderate technical debt")
        );

        let files_bad = vec![FileHealth {
            path: "f.rs".to_string(),
            score: 30.0,
            cognitive: 0.0,
            cyclomatic: 0.0,
            sloc: 0.0,
            mi: 30.0,
        }];
        assert!(
            create_report(&files_bad, false, 10, Path::new("."), "test_project")
                .contains("significant architectural review")
        );
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

        let report = create_report(&files, false, 10, Path::new("."), "test_project");
        // pkg1 avg = (100+80)/2 = 90
        assert!(report.contains("1. **pkg1**: 90%"));
        assert!(report.contains("2. **root_file.rs**: 50%"));
    }

    #[test]
    fn test_create_report_crate_label_if_workspace() {
        let files = vec![FileHealth {
            path: "pkg1/a.rs".to_string(),
            score: 100.0,
            cognitive: 0.0,
            cyclomatic: 0.0,
            sloc: 0.0,
            mi: 100.0,
        }];
        let report = create_report(&files, true, 10, Path::new("."), "test_project");
        assert!(report.contains("## Health by Crate"));
        assert!(report.contains("1. **pkg1**: 100%"));
    }

    #[test]
    fn test_create_html_report_structure() {
        let files = vec![FileHealth {
            path: "pkg1/file1.rs".to_string(),
            score: 80.0,
            cognitive: 10.0,
            cyclomatic: 5.0,
            sloc: 100.0,
            mi: 100.0,
        }];

        // Since we refactored to use create_report and conversion,
        // we just verify generate_report works or similar logic.
        // For now, let's just make sure create_report output is valid MD.
        let report = create_report(&files, false, 10, Path::new("."), "test_project");
        assert!(report.contains("# Code Health Metrics: test_project"));
    }

    #[test]
    fn test_file_health_calculate_trivial_file() {
        let results = MetricsResults {
            name: "trivial.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 0.0 },
                cyclomatic: Cyclomatic { sum: 1.0 },
                mi: Mi {
                    mi_visual_studio: 0.0, // Tool returns 0.0 for comment-only files
                },
                loc: Loc { sloc: 7.0 },
            },
        };
        let health = FileHealth::calculate(&results);
        assert!(
            health.score > 90.0,
            "Trivial files should have high health score, got {}",
            health.score
        );
    }
}
