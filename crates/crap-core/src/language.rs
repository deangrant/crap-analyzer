//! Language-specific discovery and complexity collection.

use crate::error::Result;
use crate::merge::{LocatedFn, MissingPolicy};
use crate::metric::Metric;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Options for one analysis run, shared by every language frontend.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanRequest {
    /// Walk root, or language workspace root when the frontend selects packages.
    pub path: PathBuf,
    /// Coverage file path (format is frontend-local).
    pub coverage: PathBuf,
    /// Complexity metric.
    pub metric: Metric,
    /// Score above which a function is flagged.
    pub threshold: Option<f64>,
    /// Print counts only.
    pub summary: bool,
    /// Exit 1 when any function exceeds `threshold`.
    pub fail_above: bool,
    /// Policy for functions with no coverage data.
    pub missing: MissingPolicy,
    /// Text table or JSON envelope.
    pub format: ReportFormat,
}

/// How the finished report is written to stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    /// Human table or `--summary` counts.
    Text,
    /// Versioned JSON envelope.
    Json,
}

impl FromStr for ReportFormat {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!("invalid --format `{value}`")),
        }
    }
}

impl fmt::Display for ReportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Text => "text",
            Self::Json => "json",
        })
    }
}

impl ScanRequest {
    /// Effective gate: explicit threshold, else the metric default.
    #[must_use]
    pub fn effective_threshold(&self) -> f64 {
        self.threshold.unwrap_or_else(|| self.metric.default_threshold())
    }
}

/// One source tree a language frontend asked the pipeline to analyze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Directory to walk for sources.
    pub root: PathBuf,
    /// Package name when a workspace member was selected.
    pub crate_name: Option<String>,
    /// Nested roots the walker must not enter.
    pub skip: Vec<PathBuf>,
    /// Feature names treated as enabled when evaluating `#[cfg]`.
    pub enabled_features: Vec<String>,
}

/// Discovers analysis targets and collects per-function complexity.
pub trait Language {
    /// Resolves walk roots from `request`.
    ///
    /// # Errors
    ///
    /// Returns usage, I/O, or language-workspace errors.
    fn resolve_targets(&self, request: &ScanRequest) -> Result<Vec<Target>>;

    /// Collects functions from `targets` under `metric`.
    ///
    /// # Errors
    ///
    /// Returns I/O or collect errors. Any source file that fails to parse or
    /// read fails the run.
    fn collect_functions(&self, targets: &[Target], metric: Metric) -> Result<Vec<LocatedFn>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::assert_f64_bits_eq;

    #[test]
    fn omitted_threshold_uses_metric_default() {
        let request = ScanRequest {
            path: PathBuf::from("."),
            coverage: PathBuf::from("lcov.info"),
            metric: Metric::Cyclomatic,
            threshold: None,
            summary: false,
            fail_above: false,
            missing: MissingPolicy::Pessimistic,
            format: ReportFormat::Text,
        };
        assert_f64_bits_eq(request.effective_threshold(), 15.0);
    }

    #[test]
    fn explicit_threshold_wins() {
        let request = ScanRequest {
            path: PathBuf::from("."),
            coverage: PathBuf::from("lcov.info"),
            metric: Metric::Cognitive,
            threshold: Some(8.0),
            summary: false,
            fail_above: false,
            missing: MissingPolicy::Pessimistic,
            format: ReportFormat::Text,
        };
        assert_f64_bits_eq(request.effective_threshold(), 8.0);
    }

    #[test]
    fn parses_and_displays_report_format() {
        let parsed = [
            ("text", Some(ReportFormat::Text)),
            ("json", Some(ReportFormat::Json)),
            ("nope", None),
        ];
        for (input, expected) in parsed {
            assert_eq!(input.parse::<ReportFormat>().ok(), expected);
        }
        for format in [ReportFormat::Text, ReportFormat::Json] {
            assert_eq!(
                format.to_string().parse::<ReportFormat>().ok(),
                Some(format)
            );
        }
    }
}
