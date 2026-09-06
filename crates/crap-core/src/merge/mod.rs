//! Join function spans with LCOV line hits and score them.

mod path_index;

use crate::coverage::FileCoverage;
use crate::score::crap;
use path_index::PathIndex;
use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasher;
use std::path::PathBuf;
use std::str::FromStr;

/// How to treat a function with no matching coverage data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingPolicy {
    /// Score as 0% covered.
    Pessimistic,
    /// Score as 100% covered.
    Optimistic,
    /// Drop the function from the report.
    Skip,
}

impl FromStr for MissingPolicy {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "pessimistic" => Ok(Self::Pessimistic),
            "optimistic" => Ok(Self::Optimistic),
            "skip" => Ok(Self::Skip),
            _ => Err(format!("invalid --missing `{value}`")),
        }
    }
}

impl fmt::Display for MissingPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pessimistic => "pessimistic",
            Self::Optimistic => "optimistic",
            Self::Skip => "skip",
        })
    }
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
    /// One-based last line of the function body.
    pub end_line: usize,
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
        let Some(coverage_pct) = coverage_for(&index, item, functions, missing) else {
            continue;
        };
        let cc = item.function.complexity as f64;
        entries.push(CrapEntry {
            file: item.function.file.clone(),
            function: item.function.name.clone(),
            line: item.function.start_line,
            end_line: item.function.end_line,
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
    item: &LocatedFn,
    functions: &[LocatedFn],
    missing: MissingPolicy,
) -> Option<f64> {
    let exclude = nested_excludes(functions, &item.function);
    let found = index.lookup(&item.function.file, item.crate_name.as_deref()).and_then(|file| {
        file.coverage_in_span_excluding(item.function.start_line, item.function.end_line, &exclude)
    });
    found.or(match missing {
        MissingPolicy::Pessimistic => Some(0.0),
        MissingPolicy::Optimistic => Some(100.0),
        MissingPolicy::Skip => None,
    })
}

fn nested_excludes(functions: &[LocatedFn], current: &FunctionComplexity) -> Vec<(usize, usize)> {
    functions
        .iter()
        .filter(|other| other.function.file == current.file && is_nested(current, &other.function))
        .map(|other| (other.function.start_line, other.function.end_line))
        .collect()
}

const fn is_nested(outer: &FunctionComplexity, inner: &FunctionComplexity) -> bool {
    let strictly_smaller = inner.start_line > outer.start_line || inner.end_line < outer.end_line;
    outer.start_line <= inner.start_line && inner.end_line <= outer.end_line && strictly_smaller
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
        func_in(file, name, start, end, None)
    }

    fn func_in(
        file: &str,
        name: &str,
        start: usize,
        end: usize,
        crate_name: Option<&str>,
    ) -> LocatedFn {
        LocatedFn {
            function: FunctionComplexity {
                file: PathBuf::from(file),
                name: name.into(),
                start_line: start,
                end_line: end,
                complexity: 1,
            },
            crate_name: crate_name.map(str::to_owned),
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
        assert_eq!(entries[0].line, 1);
        assert_eq!(entries[0].end_line, 1);
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
    fn crate_name_breaks_equal_length_suffix_tie() {
        let functions = [func_in("src/lib.rs", "f", 1, 1, Some("crate_a"))];
        let mut coverage = cov("/crate_a/src/lib.rs", &[(1, 1)]);
        coverage.insert(
            PathBuf::from("/crate_b/src/lib.rs"),
            FileCoverage {
                lines: std::iter::once((1, 0)).collect(),
            },
        );
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 100.0);
    }

    #[test]
    fn nested_fn_lines_are_excluded_from_outer_coverage() {
        let functions = [
            func("src/foo.rs", "outer", 1, 20),
            func("src/foo.rs", "inner", 5, 12),
        ];
        let coverage = cov(
            "src/foo.rs",
            &[(2, 1), (3, 1), (5, 0), (8, 0), (12, 0), (15, 1), (18, 1)],
        );
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        let outer = entries.iter().find(|e| e.function == "outer");
        let inner = entries.iter().find(|e| e.function == "inner");
        assert!(outer.is_some() && inner.is_some());
        let Some(outer) = outer else {
            return;
        };
        let Some(inner) = inner else {
            return;
        };
        assert_eq!(outer.coverage, 100.0);
        assert_eq!(inner.coverage, 0.0);
        assert_eq!(outer.crap, 1.0);
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
    fn parses_and_displays_missing_policy() {
        assert_eq!(
            "pessimistic".parse::<MissingPolicy>().ok(),
            Some(MissingPolicy::Pessimistic)
        );
        assert_eq!(
            "optimistic".parse::<MissingPolicy>().ok(),
            Some(MissingPolicy::Optimistic)
        );
        assert_eq!(
            "skip".parse::<MissingPolicy>().ok(),
            Some(MissingPolicy::Skip)
        );
        assert!("nope".parse::<MissingPolicy>().is_err());
        assert_eq!(MissingPolicy::Pessimistic.to_string(), "pessimistic");
        assert_eq!(MissingPolicy::Optimistic.to_string(), "optimistic");
        assert_eq!(MissingPolicy::Skip.to_string(), "skip");
    }

    #[test]
    fn leading_parent_dir_stays_on_the_stack() {
        let functions = [func("/src/lib.rs", "f", 1, 1)];
        let coverage = cov("../src/lib.rs", &[(1, 1)]);
        let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
        assert_eq!(entries[0].coverage, 100.0);
    }
}
