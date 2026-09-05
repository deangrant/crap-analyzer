//! Parse LCOV line-hit records into a per-file map.

use crate::error::{Error, Result};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Instrumented line hits for one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileCoverage {
    /// One-based line number to hit count.
    pub lines: BTreeMap<u32, u64>,
}

impl FileCoverage {
    /// Unions lines and saturating-adds overlapping hit counts.
    pub fn merge_from(&mut self, other: &Self) {
        for (&line, &hits) in &other.lines {
            let slot = self.lines.entry(line).or_insert(0);
            *slot = slot.saturating_add(hits);
        }
    }

    /// Percent of instrumented lines in `start..=end` that were hit.
    ///
    /// Returns [`None`] when the span has no instrumented lines.
    #[must_use]
    pub fn coverage_in_span(&self, start: usize, end: usize) -> Option<f64> {
        let start = u32::try_from(start).unwrap_or(u32::MAX);
        let end = u32::try_from(end).unwrap_or(u32::MAX);
        let executable: Vec<u64> = self.lines.range(start..=end).map(|(_, h)| *h).collect();
        if executable.is_empty() {
            return None;
        }
        let covered = executable.iter().filter(|hits| **hits > 0).count();
        Some((covered as f64 / executable.len() as f64) * 100.0)
    }
}

/// Reads an LCOV file from `path`.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read.
pub fn parse_lcov(path: &Path) -> Result<HashMap<PathBuf, FileCoverage>> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
    Ok(parse_lcov_text(&text))
}

/// Parses LCOV text. Unknown record types are ignored.
#[must_use]
fn parse_lcov_text(text: &str) -> HashMap<PathBuf, FileCoverage> {
    let mut files = HashMap::new();
    let mut current: Option<PathBuf> = None;
    for raw in text.lines() {
        apply_record(raw, &mut files, &mut current);
    }
    files
}

fn apply_record(
    raw: &str,
    files: &mut HashMap<PathBuf, FileCoverage>,
    current: &mut Option<PathBuf>,
) {
    if raw == "end_of_record" {
        *current = None;
        return;
    }
    if let Some(path) = raw.strip_prefix("SF:") {
        let path = PathBuf::from(path);
        files.entry(path.clone()).or_default();
        *current = Some(path);
        return;
    }
    let Some(rest) = raw.strip_prefix("DA:") else {
        return;
    };
    let Some(path) = current.as_ref() else {
        return;
    };
    let Some((line, hits)) = parse_da(rest) else {
        return;
    };
    if let Some(file) = files.get_mut(path) {
        let slot = file.lines.entry(line).or_insert(0);
        *slot = slot.saturating_add(hits);
    }
}

fn parse_da(rest: &str) -> Option<(u32, u64)> {
    let mut parts = rest.split(',');
    let line = parts.next()?.parse().ok()?;
    let hits = parts.next()?.parse().ok()?;
    Some((line, hits))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse(text: &str) -> HashMap<PathBuf, FileCoverage> {
        parse_lcov_text(text)
    }

    #[test]
    fn reads_hit_counts_for_one_file() {
        let map = parse("TN:\nSF:src/foo.rs\nDA:10,3\nDA:11,0\nend_of_record\n");
        let cov = &map[Path::new("src/foo.rs")];
        assert_eq!(cov.lines[&10], 3);
        assert_eq!(cov.lines[&11], 0);
    }

    #[test]
    fn sums_duplicate_line_records() {
        let map = parse("SF:src/foo.rs\nDA:10,2\nDA:10,3\nend_of_record\n");
        assert_eq!(map[Path::new("src/foo.rs")].lines[&10], 5);
    }

    #[test]
    fn isolates_source_files() {
        let map = parse(concat!(
            "SF:src/a.rs\nDA:1,1\nend_of_record\n",
            "SF:src/b.rs\nDA:2,4\nend_of_record\n",
        ));
        assert_eq!(map[Path::new("src/a.rs")].lines[&1], 1);
        assert_eq!(map[Path::new("src/b.rs")].lines[&2], 4);
        assert!(!map[Path::new("src/b.rs")].lines.contains_key(&1));
    }

    #[test]
    fn stray_da_after_end_is_dropped() {
        let map = parse(concat!(
            "SF:src/a.rs\nDA:1,1\nend_of_record\n",
            "DA:99,99\n",
            "SF:src/b.rs\nDA:2,4\nend_of_record\n",
        ));
        assert!(!map[Path::new("src/a.rs")].lines.contains_key(&99));
    }

    #[test]
    fn unknown_records_are_ignored() {
        let map = parse("VER:2\nSF:src/foo.rs\nDA:10,3\nFNL:0,1\nend_of_record\n");
        assert_eq!(map[Path::new("src/foo.rs")].lines[&10], 3);
    }

    #[test]
    fn empty_span_is_missing() {
        let cov = FileCoverage {
            lines: [(5, 1), (25, 1)].into_iter().collect(),
        };
        assert_eq!(cov.coverage_in_span(10, 20), None);
    }

    #[test]
    fn half_the_lines_hit_is_fifty_percent() {
        let cov = FileCoverage {
            lines: [(10, 5), (11, 0), (12, 1), (13, 0)].into_iter().collect(),
        };
        assert_eq!(cov.coverage_in_span(10, 13), Some(50.0));
    }

    #[test]
    fn merge_from_unions_and_sums() {
        let mut a = FileCoverage {
            lines: [(1, 2), (3, 0)].into_iter().collect(),
        };
        let b = FileCoverage {
            lines: [(1, 3), (2, 1)].into_iter().collect(),
        };
        a.merge_from(&b);
        assert_eq!(a.lines.get(&1), Some(&5));
        assert_eq!(a.lines.get(&2), Some(&1));
        assert_eq!(a.lines.get(&3), Some(&0));
    }
}
