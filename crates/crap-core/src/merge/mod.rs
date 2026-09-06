//! Join function spans with LCOV line hits and score them.

mod path_index;

use crate::coverage::FileCoverage;
use crate::score::crap;
use clap::ValueEnum;
use path_index::PathIndex;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::PathBuf;

/// How to treat a function with no matching coverage data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MissingPolicy {
    /// Score as 0% covered.
    Pessimistic,
    /// Score as 100% covered.
    Optimistic,
    /// Drop the function from the report.
    Skip,
}

/// One scored function after the coverage join.
#[derive(Debug, Clone, PartialEq)]
pub struct CrapEntry {
    /// Source file from the walker.
    pub file: PathBuf,
    /// Function or `Type::method` name.
    pub function: String,
    /// One-based start line.
    pub line: usize,
    /// Complexity under the selected metric.
    pub complexity: usize,
    /// Coverage percent in `[0, 100]`.
    pub coverage: f64,
    /// Combined change-risk score.
    pub crap: f64,
    /// Package name when a workspace member was selected.
    pub crate_name: Option<String>,
}

/// One function's complexity and inclusive line span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionComplexity {
    /// Source path as supplied to the walker.
    pub file: PathBuf,
    /// Free function name, or `Type::method` for impl and trait methods.
    pub name: String,
    /// One-based first line of the function.
    pub start_line: usize,
    /// One-based last line of the function body.
    pub end_line: usize,
    /// Selected metric value (cyclomatic minimum 1; cognitive may be 0).
    pub complexity: usize,
}

/// A function plus the crate it was walked from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedFn {
    /// Complexity row.
    pub function: FunctionComplexity,
    /// Package name, if known.
    pub crate_name: Option<String>,
}

/// Builds scored entries from `functions` and `coverage`.
#[must_use]
pub fn join<S: BuildHasher>(
    functions: &[LocatedFn],
    coverage: &HashMap<PathBuf, FileCoverage, S>,
    missing: MissingPolicy,
) -> Vec<CrapEntry> {
    let index = PathIndex::from_coverage(coverage);
    let mut entries = Vec::new();
    for item in functions {
        let Some(coverage_pct) = coverage_for(&index, &item.function, missing) else {
            continue;
        };
        let cc = item.function.complexity as f64;
        entries.push(CrapEntry {
            file: item.function.file.clone(),
            function: item.function.name.clone(),
            line: item.function.start_line,
            complexity: item.function.complexity,
            coverage: coverage_pct,
            crap: crap(cc, coverage_pct),
            crate_name: item.crate_name.clone(),
        });
    }
    entries.sort_by(|a, b| {
        b.crap
            .total_cmp(&a.crap)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    entries
}

fn coverage_for(
    index: &PathIndex,
    function: &FunctionComplexity,
    missing: MissingPolicy,
) -> Option<f64> {
    let found = index
        .lookup(&function.file)
        .and_then(|file| file.coverage_in_span(function.start_line, function.end_line));
    found.or(match missing {
        MissingPolicy::Pessimistic => Some(0.0),
        MissingPolicy::Optimistic => Some(100.0),
        MissingPolicy::Skip => None,
    })
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "joined coverage is a ratio of integer line counts"
)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn func(file: &str, name: &str, start: usize, end: usize) -> LocatedFn {
        LocatedFn {
            function: FunctionComplexity {
                file: PathBuf::from(file),
                name: name.into(),
                start_line: start,
                end_line: end,
                complexity: 1,
            },
            crate_name: None,
        }
    }

    fn cov(path: &str, lines: &[(u32, u64)]) -> HashMap<PathBuf, FileCoverage> {
        let mut map = HashMap::new();
        map.insert(
            PathBuf::from(path),
            FileCoverage {
                lines: lines.iter().copied().collect(),
            },
        );
        map
    }

    #[test]
    fn relative_lcov_suffix_matches_absolute_source() {
        let functions = [func("/proj/src/foo.rs", "f", 10, 12)];
        let coverage = cov("src/foo.rs", &[(10, 1), (11, 0), (12, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries.len(), 1);
        assert!((entries[0].coverage - 2.0 * 100.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn longest_suffix_wins() {
        let functions = [func("/proj/src/lib.rs", "f", 1, 1)];
        let mut coverage = cov("src/lib.rs", &[(1, 1)]);
        coverage.insert(
            PathBuf::from("vendor/dep/src/lib.rs"),
            FileCoverage {
                lines: std::iter::once((1, 0)).collect(),
            },
        );
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 100.0);
    }

    #[test]
    fn relative_keys_are_not_resolved_against_cwd() {
        let functions = [func("/other/src/foo.rs", "f", 1, 1)];
        let coverage = cov("src/foo.rs", &[(1, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 100.0);
        assert!(
            !std::env::current_dir()
                .is_ok_and(|cwd| { coverage.contains_key(&cwd.join("src/foo.rs")) })
        );
    }

    #[test]
    fn missing_pessimistic_is_zero() {
        let functions = [func("src/gone.rs", "f", 1, 1)];
        let entries = join(&functions, &HashMap::new(), MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 0.0);
        assert_eq!(entries[0].crap, 2.0);
    }

    #[test]
    fn missing_skip_drops_the_row() {
        let functions = [func("src/gone.rs", "f", 1, 1)];
        let entries = join(&functions, &HashMap::new(), MissingPolicy::Skip);
        assert!(entries.is_empty());
    }

    #[test]
    fn foosrc_does_not_match_src() {
        let functions = [func("/proj/foosrc/lib.rs", "f", 1, 1)];
        let coverage = cov("src/lib.rs", &[(1, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 0.0);
    }

    #[test]
    fn empty_span_is_pessimistic_zero() {
        let functions = [func("src/foo.rs", "f", 10, 12)];
        let coverage = cov("src/foo.rs", &[(1, 1), (20, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 0.0);
        assert_eq!(entries[0].crap, 2.0);
    }

    #[test]
    fn empty_span_skip_drops_the_row() {
        let functions = [func("src/foo.rs", "f", 10, 12)];
        let coverage = cov("src/foo.rs", &[(1, 1), (20, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Skip);
        assert!(entries.is_empty());
    }

    #[test]
    fn empty_span_optimistic_is_full() {
        let functions = [func("src/foo.rs", "f", 10, 12)];
        let coverage = cov("src/foo.rs", &[(1, 1), (20, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Optimistic);
        assert_eq!(entries[0].coverage, 100.0);
    }

    #[test]
    fn parent_dir_resolves_without_merging_unrelated_keys() {
        let functions = [func("/proj/b/src/lib.rs", "f", 1, 2)];
        let mut coverage = cov("a/../b/src/lib.rs", &[(1, 1)]);
        coverage.insert(
            PathBuf::from("a/b/src/lib.rs"),
            FileCoverage {
                lines: std::iter::once((2, 0)).collect(),
            },
        );
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 100.0);
    }

    #[test]
    fn equal_length_crate_suffixes_are_ambiguous() {
        let functions = [func("src/lib.rs", "f", 1, 1)];
        let mut coverage = cov("/crate_a/src/lib.rs", &[(1, 1)]);
        coverage.insert(
            PathBuf::from("/crate_b/src/lib.rs"),
            FileCoverage {
                lines: std::iter::once((1, 0)).collect(),
            },
        );
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 0.0);
    }

    #[test]
    fn equal_length_crate_suffixes_skip() {
        let functions = [func("src/lib.rs", "f", 1, 1)];
        let mut coverage = cov("/crate_a/src/lib.rs", &[(1, 1)]);
        coverage.insert(
            PathBuf::from("/crate_b/src/lib.rs"),
            FileCoverage {
                lines: std::iter::once((1, 0)).collect(),
            },
        );
        let entries = join(&functions, &coverage, MissingPolicy::Skip);
        assert!(entries.is_empty());
    }

    #[test]
    fn leading_parent_dir_stays_on_the_stack() {
        let functions = [func("/src/lib.rs", "f", 1, 1)];
        let coverage = cov("../src/lib.rs", &[(1, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 100.0);
    }
}
