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

#[derive(Debug)]
pub struct FileHealth {
    pub path: String,
    pub score: f64,
    pub cognitive: f64,
    pub cyclomatic: f64,
    pub sloc: f64,
    pub mi: f64,
}

impl FileHealth {
    pub fn calculate(results: &MetricsResults) -> Self {
        let mi = results.metrics.mi.mi_visual_studio;
        let cognitive = results.metrics.cognitive.sum;
        let cyclomatic = results.metrics.cyclomatic.sum;
        let sloc = results.metrics.loc.sloc;

        let score = if mi == 0.0 && cognitive == 0.0 && cyclomatic <= 1.0 {
            100.0
        } else {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_file_health_calculate_complex() {
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
    }

    #[test]
    fn test_file_health_calculate_trivial_file() {
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
        let health = FileHealth::calculate(&results);
        assert!(health.score > 90.0);
    }
}
