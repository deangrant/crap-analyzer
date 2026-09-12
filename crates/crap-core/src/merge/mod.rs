//! Join function spans with LCOV line hits and score them.

mod path_index;

use crate::coverage::FileCoverage;
use crate::score::crap;
use path_index::{Lookup, PathIndex};
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

/// How coverage percent was obtained for a joined function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageJoin {
    /// Line hits came from a unique LCOV path match.
    Measured,
    /// No path match or an empty instrumented span; `--missing` applied.
    Missing,
    /// Unresolved equal-rank path tie; `--missing` applied.
    Ambiguous,
}

impl CoverageJoin {
    /// Stable JSON / CLI token for this join outcome.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Missing => "missing",
            Self::Ambiguous => "ambiguous",
        }
    }
}

impl fmt::Display for CoverageJoin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    /// How [`Self::coverage`] was obtained.
    pub coverage_join: CoverageJoin,
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
    /// Package name, if known (report / JSON display).
    pub crate_name: Option<String>,
    /// Path-index join key when it differs from [`Self::crate_name`].
    pub join_key: Option<String>,
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
    let excludes = precomputed_nested_excludes(&by_file);
    let mut entries = Vec::new();
    for item in functions {
        let exclude =
            excludes.get(&std::ptr::from_ref(&item.function)).map_or(&[][..], Vec::as_slice);
        let Some((coverage_pct, coverage_join)) = coverage_for(&index, item, exclude, missing)
        else {
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
            coverage_join,
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

/// Count of functions scored via `--missing` because of an unresolved path tie.
#[must_use]
pub fn ambiguous_join_count(entries: &[CrapEntry]) -> usize {
    entries
        .iter()
        .filter(|entry| entry.coverage_join == CoverageJoin::Ambiguous)
        .count()
}

/// Stderr / footer line when any joins were ambiguous; otherwise empty.
#[must_use]
pub fn ambiguous_join_warning(entries: &[CrapEntry]) -> String {
    let count = ambiguous_join_count(entries);
    if count == 0 {
        String::new()
    } else {
        format!("{count} function(s) used --missing due to ambiguous coverage paths.")
    }
}

fn coverage_for(
    index: &PathIndex,
    item: &LocatedFn,
    exclude: &[(usize, usize)],
    missing: MissingPolicy,
) -> Option<(f64, CoverageJoin)> {
    match index.lookup(
        &item.function.file,
        item.join_key.as_deref().or(item.crate_name.as_deref()),
    ) {
        Lookup::Found(file) => file
            .coverage_in_span_excluding(item.function.start_line, item.function.end_line, exclude)
            .map_or_else(
                || policy_coverage(missing, CoverageJoin::Missing),
                |pct| Some((pct, CoverageJoin::Measured)),
            ),
        Lookup::Ambiguous => policy_coverage(missing, CoverageJoin::Ambiguous),
        Lookup::Missing => policy_coverage(missing, CoverageJoin::Missing),
    }
}

const fn policy_coverage(
    missing: MissingPolicy,
    join: CoverageJoin,
) -> Option<(f64, CoverageJoin)> {
    match missing {
        MissingPolicy::Pessimistic => Some((0.0, join)),
        MissingPolicy::Optimistic => Some((100.0, join)),
        MissingPolicy::Skip => None,
    }
}

fn precomputed_nested_excludes(
    by_file: &HashMap<Vec<String>, Vec<&FunctionComplexity>>,
) -> HashMap<*const FunctionComplexity, Vec<(usize, usize)>> {
    let mut out = HashMap::new();
    for peers in by_file.values() {
        for current in peers {
            out.insert(
                std::ptr::from_ref(*current),
                merge_ranges(nested_excludes(peers, current)),
            );
        }
    }
    out
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

fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    if ranges.len() <= 1 {
        return ranges;
    }
    ranges.sort_unstable();
    let mut merged = Vec::with_capacity(ranges.len());
    let (mut start, mut end) = ranges[0];
    for &(next_start, next_end) in &ranges[1..] {
        if next_start <= end.saturating_add(1) {
            end = end.max(next_end);
        } else {
            merged.push((start, end));
            start = next_start;
            end = next_end;
        }
    }
    merged.push((start, end));
    merged
}

const fn is_nested(outer: &FunctionComplexity, inner: &FunctionComplexity) -> bool {
    let strictly_smaller = inner.start_line > outer.start_line || inner.end_line < outer.end_line;
    outer.start_line <= inner.start_line && inner.end_line <= outer.end_line && strictly_smaller
}

fn functions_by_file(functions: &[LocatedFn]) -> HashMap<Vec<String>, Vec<&FunctionComplexity>> {
    let mut by_file: HashMap<Vec<String>, Vec<&FunctionComplexity>> = HashMap::new();
    for item in functions {
        by_file
            .entry(path_index::components(&item.function.file))
            .or_default()
            .push(&item.function);
    }
    by_file
}

#[cfg(test)]
#[path = "join_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "join_path_tests.rs"]
mod path_tests;
