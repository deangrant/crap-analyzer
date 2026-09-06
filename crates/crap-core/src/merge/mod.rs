//! Join function spans with LCOV line hits and score them.

mod path_index;

use crate::coverage::FileCoverage;
use crate::score::crap;
use path_index::PathIndex;
use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
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
    pub start_line: usize,
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
    let by_file = functions_by_file(functions);
    let mut entries = Vec::new();
    for item in functions {
        let Some(coverage_pct) = coverage_for(&index, item, &by_file, missing) else {
            continue;
        };
        let cc = crate::score::to_f64(item.function.complexity);
        entries.push(CrapEntry {
            file: item.function.file.clone(),
            function: item.function.name.clone(),
            start_line: item.function.start_line,
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
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
    entries
}

fn coverage_for(
    index: &PathIndex,
    item: &LocatedFn,
    by_file: &HashMap<&Path, Vec<&FunctionComplexity>>,
    missing: MissingPolicy,
) -> Option<f64> {
    let empty = [];
    let peers = by_file.get(item.function.file.as_path()).map_or(&empty[..], Vec::as_slice);
    let exclude = nested_excludes(peers, &item.function);
    let found = index.lookup(&item.function.file, item.crate_name.as_deref()).and_then(|file| {
        file.coverage_in_span_excluding(item.function.start_line, item.function.end_line, &exclude)
    });
    found.or(match missing {
        MissingPolicy::Pessimistic => Some(0.0),
        MissingPolicy::Optimistic => Some(100.0),
        MissingPolicy::Skip => None,
    })
}

fn nested_excludes(
    fns: &[&FunctionComplexity],
    current: &FunctionComplexity,
) -> Vec<(usize, usize)> {
    fns.iter()
        .filter(|other| is_nested(current, other))
        .map(|other| (other.start_line, other.end_line))
        .collect()
}

const fn is_nested(outer: &FunctionComplexity, inner: &FunctionComplexity) -> bool {
    let strictly_smaller = inner.start_line > outer.start_line || inner.end_line < outer.end_line;
    outer.start_line <= inner.start_line && inner.end_line <= outer.end_line && strictly_smaller
}

fn functions_by_file(functions: &[LocatedFn]) -> HashMap<&Path, Vec<&FunctionComplexity>> {
    let mut by_file: HashMap<&Path, Vec<&FunctionComplexity>> = HashMap::new();
    for item in functions {
        by_file.entry(item.function.file.as_path()).or_default().push(&item.function);
    }
    by_file
}

#[cfg(test)]
#[path = "join_tests.rs"]
mod tests;
