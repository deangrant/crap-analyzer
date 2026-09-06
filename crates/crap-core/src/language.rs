//! Language-specific discovery and complexity collection.

use crate::error::Result;
use crate::merge::{LocatedFn, MissingPolicy};
use crate::metric::Metric;
use std::path::PathBuf;

/// Options for one analysis run, shared by every language frontend.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanRequest {
    /// Walk root, or language workspace root when the frontend selects packages.
    pub path: PathBuf,
    /// LCOV coverage file.
    pub lcov: PathBuf,
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
    /// Returns I/O or total-parse errors. Individual parse failures become
    /// warnings unless every file fails.
    fn collect_functions(
        &self,
        targets: &[Target],
        metric: Metric,
    ) -> Result<(Vec<LocatedFn>, Vec<String>)>;
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "effective threshold is an exact metric default"
)]
mod tests {
    use super::*;

    #[test]
    fn omitted_threshold_uses_metric_default() {
        let request = ScanRequest {
            path: PathBuf::from("."),
            lcov: PathBuf::from("lcov.info"),
            metric: Metric::Cyclomatic,
            threshold: None,
            summary: false,
            fail_above: false,
            missing: MissingPolicy::Pessimistic,
        };
        assert_eq!(request.effective_threshold(), 30.0);
    }

    #[test]
    fn explicit_threshold_wins() {
        let request = ScanRequest {
            path: PathBuf::from("."),
            lcov: PathBuf::from("lcov.info"),
            metric: Metric::Cognitive,
            threshold: Some(8.0),
            summary: false,
            fail_above: false,
            missing: MissingPolicy::Pessimistic,
        };
        assert_eq!(request.effective_threshold(), 8.0);
    }
}
