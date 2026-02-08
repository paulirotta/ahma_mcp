mod analysis;
mod models;
mod report;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use analysis::{check_dependencies, get_project_name, is_cargo_workspace, perform_analysis};
use models::{FileHealth, MetricsResults};
use report::generate_report;

#[derive(Parser, Debug)]
#[command(author, version, about = "Analyzes source code metrics and generates a health report", long_about = None)]
struct Cli {
    /// Directory to analyze (absolute or relative)
    directory: PathBuf,

    /// Output directory for analysis results
    #[arg(short, long, default_value = "analysis_results")]
    output: PathBuf,

    /// Number of issues to show in the report
    #[arg(short, long, default_value_t = 10)]
    limit: usize,

    /// Open the report automatically
    #[arg(long)]
    open: bool,

    /// Shorthand for --format html
    #[arg(long)]
    html: bool,

    /// File extensions to analyze as a comma-separated list (e.g. rs,py,js).
    /// Supported: rs, py, js, ts, tsx, c, h, cpp, cc, hpp, hh, cs, java, go, css, html.
    /// Example: --extensions rs,py,js
    #[arg(short, long, default_value = "rs", value_delimiter = ',')]
    extensions: Vec<String>,

    /// Additional paths/patterns to exclude, as a comma-separated list.
    /// Example: --exclude "**/generated/**,**/vendor/**"
    #[arg(short = 'x', long, value_delimiter = ',')]
    exclude: Vec<String>,

    /// Use raw complexity values instead of SLOC-normalized density scoring
    #[arg(long)]
    raw_complexity: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    check_dependencies()?;

    let directory = cli
        .directory
        .canonicalize()
        .context("Failed to canonicalize directory")?;
    prepare_output_directory(&cli.output)?;

    let is_workspace = is_cargo_workspace(&cli.directory);
    perform_analysis(
        &directory,
        &cli.output,
        is_workspace,
        &cli.extensions,
        &cli.exclude,
    )?;

    let mut files_health = load_metrics(&cli.output, !cli.raw_complexity)?;
    if files_health.is_empty() {
        println!("No analysis files found in {}.", cli.output.display());
        return Ok(());
    }

    sort_files_by_health(&mut files_health);

    let project_name = get_project_name(&directory);
    generate_report(
        &files_health,
        is_workspace,
        cli.limit,
        &directory,
        cli.html,
        &project_name,
    )?;

    print_report_locations(&directory, cli.html);

    if cli.open {
        open_report(&directory, cli.html)?;
    }

    Ok(())
}

fn prepare_output_directory(output: &Path) -> Result<()> {
    if output.exists() {
        println!(
            "Clearing existing analysis results in {}...",
            output.display()
        );
        let _ = fs::remove_dir_all(output);
    }
    fs::create_dir_all(output).context("Failed to create output directory")
}

fn load_metrics(output: &Path, normalized: bool) -> Result<Vec<FileHealth>> {
    let mut files_health = Vec::new();
    println!("Aggregating metrics from {}...", output.display());

    for entry in WalkDir::new(output).into_iter().filter_map(|e| e.ok()) {
        if entry.path().extension().is_some_and(|ext| ext == "toml") {
            let content = fs::read_to_string(entry.path())?;
            match toml::from_str::<MetricsResults>(&content) {
                Ok(results) => files_health.push(FileHealth::calculate(&results, normalized)),
                Err(e) => eprintln!("Error parsing {}: {}", entry.path().display(), e),
            }
        }
    }
    Ok(files_health)
}

fn sort_files_by_health(files: &mut [FileHealth]) {
    files.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap()
            .then_with(|| b.cognitive.partial_cmp(&a.cognitive).unwrap())
    });
}

fn print_report_locations(directory: &Path, html: bool) {
    println!(
        "Report generated: {}",
        directory.join("CODE_HEALTH.md").display()
    );
    if html {
        println!(
            "Report generated: {}",
            directory.join("CODE_HEALTH.html").display()
        );
    }
}

fn open_report(directory: &Path, html: bool) -> Result<()> {
    let open_path = if html {
        directory.join("CODE_HEALTH.html")
    } else {
        directory.join("CODE_HEALTH.md")
    };
    opener::open(&open_path).context("Failed to open report")
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
}
