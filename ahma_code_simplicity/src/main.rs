mod analysis;
mod models;
mod report;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use analysis::{check_dependencies, get_project_name, is_cargo_workspace, perform_analysis};
use models::{FileSimplicity, MetricsResults};
use report::{generate_ai_fix_prompt, generate_report};

#[derive(Parser, Debug)]
#[command(author, version, about = "Analyzes source code metrics and generates a simplicity report", long_about = None)]
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
    /// Default: all supported extensions.
    #[arg(
        short,
        long,
        default_value = "rs,py,js,ts,tsx,c,h,cpp,cc,hpp,hh,cs,java,go,css,html",
        value_delimiter = ','
    )]
    extensions: Vec<String>,

    /// Additional paths/patterns to exclude, as a comma-separated list.
    /// Example: --exclude "**/generated/**,**/vendor/**"
    #[arg(short = 'x', long, value_delimiter = ',')]
    exclude: Vec<String>,

    /// Output path for CODE_SIMPLICITY.md and CODE_SIMPLICITY.html files.
    /// Can be a directory (uses "CODE_SIMPLICITY" as filename) or a full path with filename.
    /// Defaults to current working directory.
    #[arg(long)]
    output_path: Option<PathBuf>,

    /// Generate an AI fix prompt for the Nth most complex file (1-indexed).
    /// When set, outputs the full simplicity report and a structured prompt
    /// instructing the AI to plan and implement a fix for that issue.
    #[arg(long)]
    ai_fix: Option<usize>,
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

    let mut files_simplicity = load_metrics(&cli.output, true)?;
    if files_simplicity.is_empty() {
        println!("No analysis files found in {}.", cli.output.display());
        return Ok(());
    }

    sort_files_by_simplicity(&mut files_simplicity);

    let project_name = get_project_name(&directory);

    // Determine the report output paths
    let report_output_dir = determine_report_output_dir(&cli.output_path)?;

    // Create report output directory if it doesn't exist
    fs::create_dir_all(&report_output_dir).context("Failed to create report output directory")?;

    generate_report(
        &files_simplicity,
        is_workspace,
        cli.limit,
        &directory,
        cli.html,
        &project_name,
        &report_output_dir,
    )?;

    print_report_locations(&report_output_dir, cli.html);

    if let Some(issue_number) = cli.ai_fix {
        let md_path = report_output_dir.join("CODE_SIMPLICITY.md");
        let report_content =
            fs::read_to_string(&md_path).context("Failed to read generated report")?;
        println!("{}", report_content);

        match generate_ai_fix_prompt(&files_simplicity, issue_number, &directory) {
            Some(prompt) => println!("\n{}", prompt),
            None => eprintln!(
                "Warning: Issue #{} is out of range (only {} files analyzed).",
                issue_number,
                files_simplicity.len()
            ),
        }
    }

    if cli.open {
        open_report(&report_output_dir, cli.html)?;
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

fn determine_report_output_dir(output_path: &Option<PathBuf>) -> Result<PathBuf> {
    let path = if let Some(p) = output_path {
        if p.is_absolute() {
            p.clone()
        } else {
            std::env::current_dir()
                .context("Failed to get current directory")?
                .join(p)
        }
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    // If path ends with a filename (has an extension or contains a dot), use its parent directory
    // Otherwise, treat it as a directory
    if path.extension().is_some()
        || path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains('.'))
    {
        path.parent().map(|p| p.to_path_buf()).ok_or_else(|| {
            anyhow::anyhow!("Invalid output path: cannot determine parent directory")
        })
    } else {
        Ok(path)
    }
}

fn load_metrics(output: &Path, normalized: bool) -> Result<Vec<FileSimplicity>> {
    let mut files_simplicity = Vec::new();
    println!("Aggregating metrics from {}...", output.display());

    for entry in WalkDir::new(output).into_iter().filter_map(|e| e.ok()) {
        if entry.path().extension().is_some_and(|ext| ext == "toml") {
            let content = fs::read_to_string(entry.path())?;
            match toml::from_str::<MetricsResults>(&content) {
                Ok(results) => {
                    files_simplicity.push(FileSimplicity::calculate(&results, normalized))
                }
                Err(e) => eprintln!("Error parsing {}: {}", entry.path().display(), e),
            }
        }
    }
    Ok(files_simplicity)
}

fn sort_files_by_simplicity(files: &mut [FileSimplicity]) {
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
        directory.join("CODE_SIMPLICITY.md").display()
    );
    if html {
        println!(
            "Report generated: {}",
            directory.join("CODE_SIMPLICITY.html").display()
        );
    }
}

fn open_report(directory: &Path, html: bool) -> Result<()> {
    let open_path = if html {
        directory.join("CODE_SIMPLICITY.html")
    } else {
        directory.join("CODE_SIMPLICITY.md")
    };
    opener::open(&open_path).context("Failed to open report")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let args = vec!["ahma_code_simplicity", ".", "--output", "results"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.directory, PathBuf::from("."));
        assert_eq!(cli.output, PathBuf::from("results"));
        assert_eq!(cli.output_path, None);
    }

    #[test]
    fn test_cli_parsing_with_output_path() {
        let args = vec![
            "ahma_code_simplicity",
            ".",
            "--output",
            "results",
            "--output-path",
            "/tmp",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.directory, PathBuf::from("."));
        assert_eq!(cli.output, PathBuf::from("results"));
        assert_eq!(cli.output_path, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn test_cli_parsing_with_ai_fix() {
        let args = vec!["ahma_code_simplicity", ".", "--ai-fix", "1"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.ai_fix, Some(1));
    }

    #[test]
    fn test_cli_parsing_without_ai_fix() {
        let args = vec!["ahma_code_simplicity", "."];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.ai_fix, None);
    }
}
