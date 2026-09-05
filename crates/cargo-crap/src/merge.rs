//! Join function spans with LCOV line hits and score them.

use crate::complexity::FunctionComplexity;
use crate::coverage::FileCoverage;
use crate::score::crap;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::BuildHasher;
use std::path::{Component, Path, PathBuf};

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

/// One scored function after the coverage join.
#[derive(Debug, Clone, PartialEq)]
pub struct CrapEntry {
    /// Source file from the walker.
    pub file: PathBuf,
    /// Function or `Type::method` name.
    pub function: String,
    /// One-based start line.
    pub line: usize,
    /// Cyclomatic complexity.
    pub cyclomatic: usize,
    /// Coverage percent in `[0, 100]`.
    pub coverage: f64,
    /// Combined change-risk score.
    pub crap: f64,
    /// Package name when a workspace member was selected.
    pub crate_name: Option<String>,
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
        let cc = item.function.cyclomatic as f64;
        entries.push(CrapEntry {
            file: item.function.file.clone(),
            function: item.function.name.clone(),
            line: item.function.start_line,
            cyclomatic: item.function.cyclomatic,
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
    index.lookup(&function.file).map_or_else(
        || match missing {
            MissingPolicy::Pessimistic => Some(0.0),
            MissingPolicy::Optimistic => Some(100.0),
            MissingPolicy::Skip => None,
        },
        |file| Some(file.coverage_in_span(function.start_line, function.end_line)),
    )
}

struct PathIndex {
    files: Vec<(Vec<String>, FileCoverage)>,
}

impl PathIndex {
    fn from_coverage<S: BuildHasher>(coverage: &HashMap<PathBuf, FileCoverage, S>) -> Self {
        let mut merged: HashMap<Vec<String>, FileCoverage> = HashMap::new();
        for (path, file) in coverage {
            let key = components(path);
            merged.entry(key).or_default().merge_from(file);
        }
        Self {
            files: merged.into_iter().collect(),
        }
    }

    fn lookup(&self, source: &Path) -> Option<&FileCoverage> {
        let src = components(source);
        let mut best: Option<(usize, &FileCoverage)> = None;
        for (key, file) in &self.files {
            if !is_suffix_pair(&src, key) {
                continue;
            }
            let len = key.len().min(src.len());
            if best.is_none_or(|(best_len, _)| len > best_len) {
                best = Some((len, file));
            }
        }
        best.map(|(_, file)| file)
    }
}

fn components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|comp| match comp {
            Component::Normal(part) => Some(os_to_string(part)),
            Component::Prefix(prefix) => Some(os_to_string(prefix.as_os_str())),
            Component::RootDir | Component::CurDir | Component::ParentDir => None,
        })
        .collect()
}

fn os_to_string(part: &OsStr) -> String {
    part.to_string_lossy().replace('\\', "/")
}

fn is_suffix_pair(a: &[String], b: &[String]) -> bool {
    a.ends_with(b) || b.ends_with(a)
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "joined coverage is a ratio of integer line counts"
)]
mod tests {
    use super::*;

    fn func(file: &str, name: &str, start: usize, end: usize) -> LocatedFn {
        LocatedFn {
            function: FunctionComplexity {
                file: PathBuf::from(file),
                name: name.into(),
                start_line: start,
                end_line: end,
                cyclomatic: 1,
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
}
