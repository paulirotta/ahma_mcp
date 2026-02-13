use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MetricsResults {
    pub name: String,
    pub metrics: Metrics,
}

#[derive(Debug, Deserialize)]
pub struct Metrics {
    pub cognitive: Cognitive,
    pub cyclomatic: Cyclomatic,
    pub mi: Mi,
    pub loc: Loc,
}

#[derive(Debug, Deserialize)]
pub struct Cognitive {
    pub sum: f64,
}

#[derive(Debug, Deserialize)]
pub struct Cyclomatic {
    pub sum: f64,
}

#[derive(Debug, Deserialize)]
pub struct Mi {
    pub mi_visual_studio: f64,
}

#[derive(Debug, Deserialize)]
pub struct Loc {
    pub sloc: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Cpp,
    C,
    Java,
    CSharp,
    Go,
    Html,
    Css,
    Unknown,
}

impl Language {
    pub fn from_path(path: &std::path::Path) -> Self {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("rs") => Language::Rust,
            Some("py") => Language::Python,
            Some("js") | Some("jsx") => Language::JavaScript,
            Some("ts") | Some("tsx") => Language::TypeScript,
            Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") | Some("hxx") | Some("hh") => {
                Language::Cpp
            }
            Some("c") | Some("h") => Language::C,
            Some("java") => Language::Java,
            Some("cs") => Language::CSharp,
            Some("go") => Language::Go,
            Some("html") | Some("htm") => Language::Html,
            Some("css") => Language::Css,
            _ => Language::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Cpp => "C++",
            Language::C => "C",
            Language::Java => "Java",
            Language::CSharp => "C#",
            Language::Go => "Go",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Unknown => "Unknown",
        }
    }
}

#[derive(Debug)]
pub struct FileSimplicity {
    pub path: String,
    pub language: Language,
    pub score: f64,
    pub cognitive: f64,
    pub cyclomatic: f64,
    pub sloc: f64,
    pub mi: f64,
}

impl FileSimplicity {
    /// Calculates a simplicity score (0-100) for a file based on code metrics.
    ///
    /// # Scoring Formula
    ///
    /// ```text
    /// Score = 0.6 × MI + 0.2 × Cog_Score + 0.2 × Cyc_Score
    /// ```
    ///
    /// Where:
    /// - **MI**: Maintainability Index (Visual Studio variant, 0-100, higher = better)
    /// - **Cog_Score**: `(100 - (cognitive / sloc * 100)).max(0)` - density-based cognitive complexity
    /// - **Cyc_Score**: `(100 - (cyclomatic / sloc * 100)).max(0)` - density-based cyclomatic complexity
    ///
    /// # Weighting Rationale
    ///
    /// - MI (60%): Composite metric already balancing complexity, volume, and LOC
    /// - Cognitive (20%): Human comprehension difficulty (normalized by size)
    /// - Cyclomatic (20%): Testability (normalized by size)
    ///
    /// # Normalization
    ///
    /// Complexity is normalized by SLOC to calculate "complexity density" per 100 lines.
    /// A file with 1 complexity point per line (density = 100 per 100 lines) receives a 0 component score.
    pub fn calculate(results: &MetricsResults, normalized: bool) -> Self {
        let mi = results.metrics.mi.mi_visual_studio;
        let cognitive = results.metrics.cognitive.sum;
        let cyclomatic = results.metrics.cyclomatic.sum;
        let sloc = results.metrics.loc.sloc;

        let score = if mi == 0.0 && cognitive == 0.0 && cyclomatic <= 1.0 {
            // Trivial/empty files get perfect score
            100.0
        } else {
            let mi_score = mi.clamp(0.0, 100.0);

            let (cog_score, cyc_score) = if normalized {
                // Normalize complexity by SLOC (points per 100 lines)
                let sloc_factor = sloc.max(1.0);
                let cog_density = (cognitive / sloc_factor) * 100.0;
                let cyc_density = (cyclomatic / sloc_factor) * 100.0;

                (
                    (100.0 - cog_density).max(0.0),
                    (100.0 - cyc_density).max(0.0),
                )
            } else {
                ((100.0 - cognitive).max(0.0), (100.0 - cyclomatic).max(0.0))
            };

            0.6 * mi_score + 0.2 * cog_score + 0.2 * cyc_score
        };

        let path_obj = std::path::Path::new(&results.name);
        Self {
            path: results.name.clone(),
            language: Language::from_path(path_obj),
            score: score.clamp(0.0, 100.0),
            cognitive,
            cyclomatic,
            sloc,
            mi,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_simplicity_calculate_perfect() {
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
        let simplicity = FileSimplicity::calculate(&results, true);
        assert_eq!(simplicity.score, 100.0);
    }

    #[test]
    fn test_file_simplicity_calculate_complex_large_file() {
        let results = MetricsResults {
            name: "complex.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 50.0 },
                cyclomatic: Cyclomatic { sum: 50.0 },
                mi: Mi {
                    mi_visual_studio: 60.0,
                },
                loc: Loc { sloc: 500.0 },
            },
        };
        let simplicity = FileSimplicity::calculate(&results, true);
        // mi_score=60, cog_density=10, cyc_density=10 => cog_score=90, cyc_score=90
        // 0.6*60 + 0.2*90 + 0.2*90 = 36 + 18 + 18 = 72
        assert_eq!(simplicity.score, 72.0);
    }

    #[test]
    fn test_file_simplicity_calculate_complex_raw() {
        let results = MetricsResults {
            name: "complex.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 50.0 },
                cyclomatic: Cyclomatic { sum: 50.0 },
                mi: Mi {
                    mi_visual_studio: 60.0,
                },
                loc: Loc { sloc: 500.0 },
            },
        };
        let simplicity = FileSimplicity::calculate(&results, false);
        // mi_score=60, cog_score=50, cyc_score=50
        // 0.6*60 + 0.2*50 + 0.2*50 = 36 + 10 + 10 = 56
        assert_eq!(simplicity.score, 56.0);
    }

    #[test]
    fn test_file_simplicity_calculate_high_density() {
        let results = MetricsResults {
            name: "dense.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 50.0 },
                cyclomatic: Cyclomatic { sum: 50.0 },
                mi: Mi {
                    mi_visual_studio: 60.0,
                },
                loc: Loc { sloc: 50.0 },
            },
        };
        let simplicity = FileSimplicity::calculate(&results, true);
        // mi_score=60, cog_density=100, cyc_density=100 => cog_score=0, cyc_score=0
        // 0.6*60 + 0.2*0 + 0.2*0 = 36
        assert_eq!(simplicity.score, 36.0);
    }

    #[test]
    fn test_file_simplicity_calculate_trivial_file() {
        let results = MetricsResults {
            name: "trivial.rs".to_string(),
            metrics: Metrics {
                cognitive: Cognitive { sum: 0.0 },
                cyclomatic: Cyclomatic { sum: 1.0 },
                mi: Mi {
                    mi_visual_studio: 0.0,
                },
                loc: Loc { sloc: 7.0 },
            },
        };
        let simplicity = FileSimplicity::calculate(&results, true);
        assert!(simplicity.score > 90.0);
    }
}
