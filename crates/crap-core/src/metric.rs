//! Complexity metric selection shared by language frontends.

use std::fmt;
use std::str::FromStr;

/// Which complexity metric to apply to each function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl FromStr for Metric {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "cyclomatic" => Ok(Self::Cyclomatic),
            "cognitive" => Ok(Self::Cognitive),
            _ => Err(format!("invalid --metric `{value}`")),
        }
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cyclomatic => "cyclomatic",
            Self::Cognitive => "cognitive",
        })
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

    #[test]
    fn parses_metric_names() {
        assert_eq!(
            "cyclomatic".parse::<Metric>().ok(),
            Some(Metric::Cyclomatic)
        );
        assert_eq!("cognitive".parse::<Metric>().ok(), Some(Metric::Cognitive));
        assert!("nope".parse::<Metric>().is_err());
    }

    #[test]
    fn displays_metric_names() {
        assert_eq!(Metric::Cyclomatic.to_string(), "cyclomatic");
        assert_eq!(Metric::Cognitive.to_string(), "cognitive");
    }
}
