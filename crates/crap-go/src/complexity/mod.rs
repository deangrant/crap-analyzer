//! Complexity metrics and source spans for Go functions.

mod cognitive;
mod cyclomatic;
mod lex;
mod visitor;

use crap_core::{FunctionComplexity, Metric};
use std::path::Path;
use visitor::analyze_source as analyze_source_inner;

/// Parses `source` as if it lived at `path`.
#[must_use]
pub fn analyze_source(path: &Path, source: &str, metric: Metric) -> Vec<FunctionComplexity> {
    analyze_source_inner(path, source, metric)
}

fn count_metric(metric: Metric, body: &str) -> usize {
    match metric {
        Metric::Cyclomatic => cyclomatic::count(body),
        Metric::Cognitive => cognitive::count(body),
    }
}

#[cfg(test)]
mod tests {
    use crate::complexity::analyze_source;
    use crap_core::Metric;
    use std::path::Path;

    #[test]
    fn cognitive_metric_counts_nested_if() {
        let src = "package p\nfunc f(x int) { if x > 0 { if x > 1 { x } } }\n";
        let fns = analyze_source(Path::new("t.go"), src, Metric::Cognitive);
        assert_eq!(fns[0].complexity, 3);
    }
}
