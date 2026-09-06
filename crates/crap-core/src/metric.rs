//! Complexity metric selection shared by language frontends.

use clap::ValueEnum;

/// Which complexity metric to apply to each function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Metric {
    /// Cyclomatic complexity (one plus each decision point).
    Cyclomatic,
    /// Cognitive complexity (nesting-weighted control flow).
    Cognitive,
}

impl Metric {
    /// Default CRAP gate when `--threshold` is omitted.
    #[must_use]
    pub const fn default_threshold(self) -> f64 {
        match self {
            Self::Cyclomatic => 30.0,
            Self::Cognitive => 15.0,
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "metric default thresholds are exact literals"
)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_differ_by_metric() {
        assert_eq!(Metric::Cyclomatic.default_threshold(), 30.0);
        assert_eq!(Metric::Cognitive.default_threshold(), 15.0);
    }
}
