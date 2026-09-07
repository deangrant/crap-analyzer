//! Parse Go coverprofile files into [`FileCoverage`].

use crap_core::{Error, FileCoverage, Result};
use std::collections::HashMap;
use std::fs::File;
use std::hash::BuildHasher;
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

/// Rewrites import-path coverprofile keys to paths under `module_root`.
///
/// Keys that begin with `{module_path}/` become `module_root.join(rest)`.
/// Other keys (absolute GOPATH paths, dependencies, bare names) stay as-is.
#[must_use]
pub fn remap_import_paths<S: BuildHasher>(
    coverage: &HashMap<PathBuf, FileCoverage, S>,
    module_root: &Path,
    module_path: &str,
) -> HashMap<PathBuf, FileCoverage> {
    let prefix = format!("{module_path}/");
    let mut out: HashMap<PathBuf, FileCoverage> = HashMap::new();
    for (path, file) in coverage {
        let key = remap_one_path(path, module_root, &prefix);
        out.entry(key).or_default().merge_from(file);
    }
    out
}

fn remap_one_path(path: &Path, module_root: &Path, prefix: &str) -> PathBuf {
    let key = path.to_string_lossy().replace('\\', "/");
    key.strip_prefix(prefix)
        .map_or_else(|| path.to_path_buf(), |rel| module_root.join(rel))
}

fn parse_reader<R: BufRead>(reader: R, origin: &Path) -> Result<HashMap<PathBuf, FileCoverage>> {
    let mut lines = reader.lines();
    let mode = read_mode(&mut lines, origin)?;
    let (files, data_lines) = collect_data_lines(&mut lines, mode, origin)?;
    if data_lines == 0 {
        return Err(Error::coverage(format!(
            "{}: coverprofile has no data lines",
            origin.display()
        )));
    }
    Ok(files)
}

fn collect_data_lines<R: BufRead>(
    lines: &mut std::io::Lines<R>,
    mode: Mode,
    origin: &Path,
) -> Result<(HashMap<PathBuf, FileCoverage>, usize)> {
    let mut files = HashMap::new();
    let mut data_lines = 0_usize;
    for raw in lines {
        let raw = raw.map_err(|source| Error::io(origin, source))?;
        if apply_data_line(raw.trim(), mode, origin, &mut files)? {
            data_lines += 1;
        }
    }
    Ok((files, data_lines))
}

fn apply_data_line(
    trimmed: &str,
    mode: Mode,
    origin: &Path,
    files: &mut HashMap<PathBuf, FileCoverage>,
) -> Result<bool> {
    if trimmed.is_empty() {
        return Ok(false);
    }
    apply_block(trimmed, mode, origin, files)?;
    Ok(true)
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
    let (hits_text, stmts_text, path_span) = split_data_fields(line)?;
    let (path, start, end) = parse_path_span(path_span)?;
    parse_hits_and_span(hits_text, stmts_text, path, start, end)
}

fn split_data_fields(line: &str) -> Option<(&str, &str, &str)> {
    let parts: Vec<&str> = line.rsplitn(3, ' ').collect();
    let [hits_text, stmts_text, path_span] = parts.as_slice() else {
        return None;
    };
    Some((*hits_text, *stmts_text, *path_span))
}

fn parse_hits_and_span(
    hits_text: &str,
    stmts_text: &str,
    path: PathBuf,
    start: u32,
    end: u32,
) -> Option<(PathBuf, u32, u32, u64)> {
    let _stmts = stmts_text.parse::<u64>().ok()?;
    let hits = hits_text.parse::<u64>().ok()?;
    valid_span(start, end).then_some((path, start, end, hits))
}

const fn valid_span(start: u32, end: u32) -> bool {
    start != 0 && end != 0 && end >= start
}

fn parse_path_span(path_span: &str) -> Option<(PathBuf, u32, u32)> {
    let (file, start_part, end_part) = split_path_span(path_span)?;
    let (start, end) = parse_span_lines(start_part, end_part)?;
    Some((PathBuf::from(file.replace('\\', "/")), start, end))
}

fn split_path_span(path_span: &str) -> Option<(&str, &str, &str)> {
    let (file, span) = path_span.rsplit_once(':')?;
    let (start_part, end_part) = span.split_once(',')?;
    Some((file, start_part, end_part))
}

fn parse_span_lines(start_part: &str, end_part: &str) -> Option<(u32, u32)> {
    Some((parse_dotted_line(start_part)?, parse_dotted_line(end_part)?))
}

fn parse_dotted_line(part: &str) -> Option<u32> {
    let (line, _) = part.split_once('.')?;
    line.parse().ok()
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
