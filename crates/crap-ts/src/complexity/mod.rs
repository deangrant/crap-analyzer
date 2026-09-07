//! Complexity metrics and source spans for TypeScript functions.

mod cognitive;
mod cyclomatic;
mod lex;
mod visitor;

use crap_core::Metric;

#[doc(inline)]
pub use visitor::analyze_source;

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
        let src = "function f(x: number) { if (x > 0) { if (x > 1) { x; } } }\n";
        let fns = analyze_source(Path::new("t.ts"), src, Metric::Cognitive);
        assert_eq!(fns[0].complexity, 3);
    }
}
