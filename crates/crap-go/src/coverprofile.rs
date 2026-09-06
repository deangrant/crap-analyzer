//! Parse Go coverprofile files into [`FileCoverage`].

use crap_core::{Error, FileCoverage, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Coverprofile aggregation mode from the `mode:` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Set,
    Count,
}

/// Reads a Go coverprofile from `path`.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, or [`Error::Coverage`]
/// if the mode line is missing/invalid or there are no data lines.
pub fn parse_coverprofile(path: &Path) -> Result<HashMap<PathBuf, FileCoverage>> {
    let file = File::open(path).map_err(|source| Error::io(path, source))?;
    parse_reader(BufReader::new(file), path)
}

fn parse_reader<R: BufRead>(reader: R, origin: &Path) -> Result<HashMap<PathBuf, FileCoverage>> {
    let mut lines = reader.lines();
    let mode = read_mode(&mut lines, origin)?;
    let mut files = HashMap::new();
    let mut data_lines = 0_usize;
    for raw in lines {
        let raw = raw.map_err(|source| Error::io(origin, source))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        apply_block(trimmed, mode, origin, &mut files)?;
        data_lines += 1;
    }
    if data_lines == 0 {
        return Err(Error::coverage(format!(
            "{}: coverprofile has no data lines",
            origin.display()
        )));
    }
    Ok(files)
}

fn read_mode<R: BufRead>(lines: &mut std::io::Lines<R>, origin: &Path) -> Result<Mode> {
    let Some(first) = lines.next() else {
        return Err(Error::coverage(format!(
            "{}: coverprofile is empty",
            origin.display()
        )));
    };
    let first = first.map_err(|source| Error::io(origin, source))?;
    parse_mode_line(first.trim(), origin)
}

fn parse_mode_line(line: &str, origin: &Path) -> Result<Mode> {
    let Some(rest) = line.strip_prefix("mode:") else {
        return Err(Error::coverage(format!(
            "{}: missing coverprofile mode line",
            origin.display()
        )));
    };
    match rest.trim() {
        "set" => Ok(Mode::Set),
        "count" | "atomic" => Ok(Mode::Count),
        other => Err(Error::coverage(format!(
            "{}: unknown coverprofile mode `{other}`",
            origin.display()
        ))),
    }
}

fn apply_block(
    line: &str,
    mode: Mode,
    origin: &Path,
    files: &mut HashMap<PathBuf, FileCoverage>,
) -> Result<()> {
    let (path, start, end, hits) = parse_data_line(line, origin)?;
    let file = files.entry(path).or_default();
    for line_no in start..=end {
        merge_hit(file, line_no, hits, mode);
    }
    Ok(())
}

fn merge_hit(file: &mut FileCoverage, line: u32, hits: u64, mode: Mode) {
    let slot = file.lines.entry(line).or_insert(0);
    match mode {
        Mode::Set => {
            if hits > 0 {
                *slot = (*slot).max(1);
            }
        }
        Mode::Count => *slot = slot.saturating_add(hits),
    }
}

fn parse_data_line(line: &str, origin: &Path) -> Result<(PathBuf, u32, u32, u64)> {
    parse_data_fields(line).map_or_else(|| malformed(origin, line), Ok)
}

fn parse_data_fields(line: &str) -> Option<(PathBuf, u32, u32, u64)> {
    // `file.go:startLine.startCol,endLine.endCol stmts hits`
    let parts: Vec<&str> = line.rsplitn(3, ' ').collect();
    let [hits_text, stmts_text, path_span] = parts.as_slice() else {
        return None;
    };
    let (path, start, end) = parse_path_span(path_span)?;
    let _stmts = stmts_text.parse::<u64>().ok()?;
    let hits = hits_text.parse::<u64>().ok()?;
    if start == 0 || end == 0 || end < start {
        return None;
    }
    Some((path, start, end, hits))
}

fn parse_path_span(path_span: &str) -> Option<(PathBuf, u32, u32)> {
    let (file, span) = path_span.rsplit_once(':')?;
    let (start_part, end_part) = span.split_once(',')?;
    let (start_line, _) = start_part.split_once('.')?;
    let (end_line, _) = end_part.split_once('.')?;
    let start = start_line.parse().ok()?;
    let end = end_line.parse().ok()?;
    Some((PathBuf::from(file.replace('\\', "/")), start, end))
}

fn malformed(origin: &Path, line: &str) -> Result<(PathBuf, u32, u32, u64)> {
    Err(Error::coverage(format!(
        "{}: malformed coverprofile line: {line}",
        origin.display()
    )))
}

#[cfg(test)]
#[path = "coverprofile_tests.rs"]
mod tests;
