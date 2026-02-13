use crate::analysis::{get_package_name, get_relative_path};
use crate::models::{FileSimplicity, Language};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct RepoSummary {
    pub avg_score: f64,
    pub language_summaries: HashMap<Language, LanguageSummary>,
}

pub struct LanguageSummary {
    pub score: f64,
    pub package_scores: Vec<(String, f64)>,
}

impl RepoSummary {
    pub fn from_files(files: &[FileSimplicity], base_dir: &Path) -> Self {
        let avg_score = if files.is_empty() {
            0.0
        } else {
            files.iter().map(|f| f.score).sum::<f64>() / files.len() as f64
        };

        let mut lang_map: HashMap<Language, Vec<&FileSimplicity>> = HashMap::new();
        for f in files {
            lang_map.entry(f.language).or_default().push(f);
        }

        let mut language_summaries = HashMap::new();

        for (lang, lang_files) in lang_map {
            let lang_avg = if lang_files.is_empty() {
                0.0
            } else {
                lang_files.iter().map(|f| f.score).sum::<f64>() / lang_files.len() as f64
            };

            let mut package_map: HashMap<String, Vec<f64>> = HashMap::new();
            for f in &lang_files {
                let package = get_package_name(Path::new(&f.path), base_dir);
                package_map.entry(package).or_default().push(f.score);
            }

            let mut package_scores: Vec<(String, f64)> = package_map
                .into_iter()
                .map(|(p, scores)| {
                    let avg = scores.iter().sum::<f64>() / scores.len() as f64;
                    (p, avg)
                })
                .collect();
            package_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

            language_summaries.insert(
                lang,
                LanguageSummary {
                    score: lang_avg,
                    package_scores,
                },
            );
        }

        Self {
            avg_score,
            language_summaries,
        }
    }
}

pub fn generate_report(
    files: &[FileSimplicity],
    is_workspace: bool,
    limit: usize,
    output_dir: &Path,
    generate_html: bool,
    project_name: &str,
    report_output_dir: &Path,
) -> Result<(), std::io::Error> {
    let md_content = create_report_md(files, is_workspace, limit, output_dir, project_name);

    fs::write(report_output_dir.join("CODE_SIMPLICITY.md"), &md_content)?;

    if generate_html {
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
            ";

        let full_html = format!(
            "<!DOCTYPE html>\n<html>\n<head>\n<meta charset='UTF-8'>\n<title>Code Simplicity Report</title>\n<style>\n{}\n</style>\n</head>\n<body>\n{}\n</body>\n</html>",
            style, html_output
        );
        fs::write(report_output_dir.join("CODE_SIMPLICITY.html"), full_html)?;
    }
    Ok(())
}

pub fn create_report_md(
    files: &[FileSimplicity],
    is_workspace: bool,
    limit: usize,
    base_dir: &Path,
    project_name: &str,
) -> String {
    let summary = RepoSummary::from_files(files, base_dir);
    let mut report = String::new();

    write_header(&mut report, project_name, summary.avg_score);
    write_executive_summary(&mut report, summary.avg_score);
    write_package_simplicity(&mut report, &summary, is_workspace);
    write_emergencies(&mut report, files, limit, base_dir);
    write_glossary(&mut report);

    report
}

fn write_header(report: &mut String, project_name: &str, avg_score: f64) {
    report.push_str(&format!("# Code Simplicity Metrics: {}\n\n", project_name));
    report.push_str(&format!(
        "## Overall Repository Simplicity: **{:.0}%**\n\n",
        avg_score
    ));
    let now = chrono::Local::now();
    report.push_str(&format!(
        "*Generated on: {}*\n\n",
        now.format("%Y-%m-%d %H:%M:%S")
    ));
}

fn write_executive_summary(report: &mut String, avg_score: f64) {
    report.push_str("### Executive Summary\n");
    let msg = if avg_score > 80.0 {
        "The repository has good simplicity overall. Focus on isolated high-complexity files.\n\n"
    } else if avg_score > 60.0 {
        "The repository has moderate technical debt. Consider refactoring the top complexity issues.\n\n"
    } else {
        "The repository requires significant architectural review. Multiple areas show high complexity.\n\n"
    };
    report.push_str(msg);
}

fn write_package_simplicity(report: &mut String, summary: &RepoSummary, is_workspace: bool) {
    // Sort languages by name for consistent output
    let mut languages: Vec<_> = summary.language_summaries.keys().collect();
    languages.sort_by(|a, b| a.display_name().cmp(b.display_name()));

    for lang in languages {
        if let Some(lang_summary) = summary.language_summaries.get(lang) {
            report.push_str(&format!(
                "## {} Simplicity (Avg: {:.0}%)\n\n",
                lang.display_name(),
                lang_summary.score
            ));

            let group_label = match lang {
                Language::Rust => {
                    if is_workspace {
                        "Crate"
                    } else {
                        "Module"
                    }
                }
                Language::Python | Language::JavaScript | Language::TypeScript => "Module",
                _ => "Directory",
            };

            if lang_summary.package_scores.len() > 1 {
                report.push_str(&format!("### By {}\n\n", group_label));

                for (i, (p, score)) in lang_summary.package_scores.iter().enumerate() {
                    report.push_str(&format!("{}. **{}**: {:.0}%\n", i + 1, p, score));
                }
                report.push('\n');
            }
        }
    }
}

fn write_emergencies(report: &mut String, files: &[FileSimplicity], limit: usize, base_dir: &Path) {
    let mut lang_map: HashMap<Language, Vec<&FileSimplicity>> = HashMap::new();
    for f in files {
        lang_map.entry(f.language).or_default().push(f);
    }

    let mut languages: Vec<_> = lang_map.keys().collect();
    languages.sort_by(|a, b| a.display_name().cmp(b.display_name()));

    for lang in languages {
        let lang_files = lang_map.get(lang).unwrap();
        let display_limit = std::cmp::min(limit, lang_files.len());

        report.push_str(&format!(
            "## Top {display_limit} {} Code Complexity Issues (Lowest Simplicity)\n\n",
            lang.display_name()
        ));

        for (i, f) in lang_files.iter().take(display_limit).enumerate() {
            let culprit = identify_culprit(f);
            let path = Path::new(&f.path);
            let rel_path = get_relative_path(path, base_dir);
            let rel_str = rel_path.to_string_lossy();
            let basename = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_str.to_string());

            report.push_str(&format!(
                "{}. **{}**: {:.0}% ({})**\n\t{}\n",
                i + 1,
                basename,
                f.score,
                culprit,
                rel_str
            ));
            report.push_str(&format!(
                "    - Metrics: Cog: {}, Cyc: {}, SLOC: {}, MI: {:.1}\n",
                f.cognitive, f.cyclomatic, f.sloc, f.mi
            ));
        }
        report.push('\n');
    }
}

fn identify_culprit(f: &FileSimplicity) -> &'static str {
    if f.cognitive > 20.0 {
        "High Cognitive Complexity"
    } else if f.cyclomatic > 20.0 {
        "High Cyclomatic Complexity"
    } else if f.sloc > 500.0 {
        "Mega-file"
    } else if f.mi < 50.0 {
        "Low Maintainability Index"
    } else {
        "General Complexity"
    }
}

fn write_glossary(report: &mut String) {
    report.push_str("\n---\n\n## Metrics Glossary\n\n");
    report.push_str("### Cognitive Complexity\n- **Description**: Measures how hard it is to understand the control flow of the code. [See](https://axify.io/blog/cognitive-complexity)\n- **How to Improve**: Extract complex conditions into well-named functions and reduce nesting levels.\n\n");
    report.push_str("### Cyclomatic Complexity\n- **Description**: Measures the number of linearly independent paths through the source code. [See](https://www.nist.gov/publications/structured-testing-software-testing-methodology-using-cyclomatic-complexity-metric)\n- **How to Improve**: Use polymorphic abstractions instead of complex switch/if-else chains, and break down large functions into smaller components.\n\n");
    report.push_str("### Source Lines of Code (SLOC)\n- **Description**: A measure of the size of the computer program by counting the number of lines in the text of the source code. [See](https://en.wikipedia.org/wiki/Source_lines_of_code)\n- **How to Improve**: Remove dead code and refactor repetitive logic into reusable helper functions or macros.\n\n");
    report.push_str("### Maintainability Index (MI)\n- **Description**: A composite metric representing the relative ease of maintaining the code; higher is better. [See](https://learn.microsoft.com/en-us/visualstudio/code-quality/code-metrics-maintainability-index-range-and-meaning)\n- **How to Improve**: Simultaneously reduce complexity (both cognitive and cyclomatic) and file size to boost the index.\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_report_structure() {
        let files = vec![
            FileSimplicity {
                path: "pkg1/file1.rs".to_string(),
                language: Language::Rust,
                score: 80.0,
                cognitive: 10.0,
                cyclomatic: 5.0,
                sloc: 100.0,
                mi: 100.0,
            },
            FileSimplicity {
                path: "pkg2/file2.rs".to_string(),
                language: Language::Rust,
                score: 40.0,
                cognitive: 30.0,
                cyclomatic: 25.0,
                sloc: 600.0,
                mi: 40.0,
            },
        ];

        let report = create_report_md(&files, false, 10, Path::new("."), "test_project");
        assert!(report.contains("# Code Simplicity Metrics: test_project"));
        assert!(report.contains("## Overall Repository Simplicity: **60%**"));
        assert!(report.contains("## Rust Simplicity"));
    }

    #[test]
    fn test_report_multi_language_emergencies() {
        let files = vec![
            FileSimplicity {
                path: "file1.rs".to_string(),
                language: Language::Rust,
                score: 50.0,
                cognitive: 20.0,
                cyclomatic: 15.0,
                sloc: 100.0,
                mi: 50.0,
            },
            FileSimplicity {
                path: "file2.py".to_string(),
                language: Language::Python,
                score: 40.0,
                cognitive: 25.0,
                cyclomatic: 20.0,
                sloc: 150.0,
                mi: 40.0,
            },
        ];

        let report = create_report_md(&files, false, 10, Path::new("."), "test_multi");

        assert!(report.contains("## Top 1 Rust Code Complexity Issues"));
        assert!(report.contains("## Top 1 Python Code Complexity Issues"));
        assert!(report.contains("file1.rs"));
        assert!(report.contains("file2.py"));
        // Ensure the redundant (Language) label is removed from the item lines
        assert!(!report.contains("(Rust)"));
        assert!(!report.contains("(Python)"));
    }
}
