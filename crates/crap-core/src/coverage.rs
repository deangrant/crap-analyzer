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
        self.coverage_in_span_excluding(start, end, &[])
    }

    /// Like [`Self::coverage_in_span`], omitting lines inside `exclude` ranges.
    ///
    /// Nested function spans use this so the outer function is not charged
    /// for the inner function's instrumented lines.
    #[must_use]
    pub fn coverage_in_span_excluding(
        &self,
        start: usize,
        end: usize,
        exclude: &[(usize, usize)],
    ) -> Option<f64> {
        let start = u32::try_from(start).unwrap_or(u32::MAX);
        let end = u32::try_from(end).unwrap_or(u32::MAX);
        let executable: Vec<u64> = self
            .lines
            .range(start..=end)
            .filter(|(line, _)| !line_in_ranges(**line, exclude))
            .map(|(_, hits)| *hits)
            .collect();
        if executable.is_empty() {
            return None;
        }
        let covered = executable.iter().filter(|hits| **hits > 0).count();
        Some((covered as f64 / executable.len() as f64) * 100.0)
    }
}

fn line_in_ranges(line: u32, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(start, end)| {
        let start = u32::try_from(start).unwrap_or(u32::MAX);
        let end = u32::try_from(end).unwrap_or(u32::MAX);
        line >= start && line <= end
    })
}

/// Reads an LCOV file from `path`.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, or [`Error::Coverage`]
/// if the file has no valid `DA:` line-hit records.
pub fn parse_lcov(path: &Path) -> Result<HashMap<PathBuf, FileCoverage>> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
    let (files, valid_da) = parse_lcov_text(&text);
    if valid_da == 0 {
        return Err(Error::coverage(format!(
            "{}: LCOV has no valid line-hit (DA) records",
            path.display()
        )));
    }
    Ok(files)
}

/// Parses LCOV text. Unknown record types are ignored.
fn parse_lcov_text(text: &str) -> (HashMap<PathBuf, FileCoverage>, usize) {
    let mut files = HashMap::new();
    let mut current: Option<PathBuf> = None;
    let mut valid_da = 0_usize;
    for raw in text.lines() {
        apply_record(raw, &mut files, &mut current, &mut valid_da);
    }
    (files, valid_da)
}

fn apply_record(
    raw: &str,
    files: &mut HashMap<PathBuf, FileCoverage>,
    current: &mut Option<PathBuf>,
    valid_da: &mut usize,
) {
    if raw == "end_of_record" {
        *current = None;
        return;
    }
    if let Some(path) = raw.strip_prefix("SF:") {
        let path = PathBuf::from(path.replace('\\', "/"));
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
    *valid_da += 1;
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
        parse_lcov_text(text).0
    }

    fn reject(text: &str) -> bool {
        parse_lcov_text(text).1 == 0
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
    fn backslash_sf_paths_are_normalized() {
        let map = parse("SF:src\\foo.rs\nDA:10,1\nend_of_record\n");
        assert_eq!(map[Path::new("src/foo.rs")].lines[&10], 1);
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
    fn excluding_nested_span_drops_those_lines() {
        let cov = FileCoverage {
            lines: [(1, 1), (5, 0), (6, 0), (10, 1)].into_iter().collect(),
        };
        assert_eq!(
            cov.coverage_in_span_excluding(1, 10, &[(5, 6)]),
            Some(100.0)
        );
        assert_eq!(cov.coverage_in_span(1, 10), Some(50.0));
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

    #[test]
    fn malformed_da_records_are_ignored() {
        let map = parse("SF:src/foo.rs\nDA:not-a-number\nDA:10\nend_of_record\n");
        assert!(map[Path::new("src/foo.rs")].lines.is_empty());
        assert!(reject(
            "SF:src/foo.rs\nDA:not-a-number\nDA:10\nend_of_record\n"
        ));
    }

    #[test]
    fn empty_or_garbage_lcov_has_no_da() {
        assert!(reject(""));
        assert!(reject("not lcov at all\n"));
        assert!(reject("SF:src/foo.rs\nend_of_record\n"));
    }

    #[test]
    fn parse_lcov_rejects_empty_file() {
        let path = std::env::temp_dir().join(format!(
            "crap-core-empty-lcov-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let written = std::fs::write(&path, "");
        assert!(written.is_ok(), "{written:?}");
        let parsed = parse_lcov(&path);
        let _ = std::fs::remove_file(&path);
        assert!(parsed.is_err(), "{parsed:?}");
        let message = parsed.as_ref().err().map_or(String::new(), ToString::to_string);
        assert!(
            message.contains("DA") || message.contains("line-hit"),
            "{message}"
        );
    }

    #[test]
    fn sparse_sf_without_da_is_ok_when_another_file_has_hits() {
        let (map, valid_da) = parse_lcov_text(concat!(
            "SF:generated.rs\nend_of_record\n",
            "SF:src/lib.rs\nDA:10,1\nend_of_record\n",
        ));
        assert_eq!(valid_da, 1);
        assert!(map[Path::new("generated.rs")].lines.is_empty());
        assert_eq!(map[Path::new("src/lib.rs")].lines[&10], 1);
    }
}
