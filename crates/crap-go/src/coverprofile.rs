//! Parse Go coverprofile files into [`FileCoverage`].

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use crap_core::{Error, FileCoverage, Result};
use std::collections::HashMap;
use std::fs::File;
use std::hash::BuildHasher;
use std::io::{BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};

/// Coverprofile aggregation mode from the `mode:` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Set,
    Count,
}

/// Per-line accumulation before pessimistic finalize.
#[derive(Debug, Default, Clone)]
struct LineAccum {
    positive: u64,
    seen_zero: bool,
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

/// Parses coverprofile bytes (for tests and fuzzing).
///
/// # Errors
///
/// Returns [`Error::Io`] on invalid UTF-8 line decoding, or [`Error::Coverage`]
/// when the mode line is missing/invalid or there are no data lines.
pub fn parse_coverprofile_bytes(bytes: &[u8]) -> Result<HashMap<PathBuf, FileCoverage>> {
    parse_reader(Cursor::new(bytes), Path::new("<bytes>"))
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
    remap_import_paths_all(
        coverage,
        &[(module_root.to_path_buf(), module_path.to_owned())],
    )
}

/// Remaps import-path keys for every module (longest module path first).
#[must_use]
pub fn remap_import_paths_all<S: BuildHasher>(
    coverage: &HashMap<PathBuf, FileCoverage, S>,
    modules: &[(PathBuf, String)],
) -> HashMap<PathBuf, FileCoverage> {
    let mut ordered: Vec<(PathBuf, String)> = modules.to_vec();
    ordered.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    let mut out: HashMap<PathBuf, FileCoverage> = HashMap::new();
    for (path, file) in coverage {
        let key = remap_against_modules(path, &ordered);
        out.entry(key).or_default().merge_from(file);
    }
    out
}

fn remap_against_modules(path: &Path, modules: &[(PathBuf, String)]) -> PathBuf {
    let key = path.to_string_lossy().replace('\\', "/");
    for (module_root, module_path) in modules {
        let prefix = format!("{module_path}/");
        if let Some(rel) = key.strip_prefix(&prefix) {
            return module_root.join(rel);
        }
    }
    path.to_path_buf()
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
    let mut accum: HashMap<PathBuf, HashMap<u32, LineAccum>> = HashMap::new();
    let mut data_lines = 0_usize;
    for raw in lines {
        let raw = raw.map_err(|source| Error::io(origin, source))?;
        if apply_data_line(raw.trim(), mode, origin, &mut accum)? {
            data_lines += 1;
        }
    }
    Ok((finalize_files(accum), data_lines))
}

fn apply_data_line(
    trimmed: &str,
    mode: Mode,
    origin: &Path,
    files: &mut HashMap<PathBuf, HashMap<u32, LineAccum>>,
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
    files: &mut HashMap<PathBuf, HashMap<u32, LineAccum>>,
) -> Result<()> {
    let (path, start, end, hits) = parse_data_line(line, origin)?;
    let file = files.entry(path).or_default();
    for line_no in start..=end {
        merge_hit(file.entry(line_no).or_default(), hits, mode);
    }
    Ok(())
}

fn merge_hit(slot: &mut LineAccum, hits: u64, mode: Mode) {
    if hits == 0 {
        slot.seen_zero = true;
        return;
    }
    match mode {
        Mode::Set => slot.positive = slot.positive.max(1),
        Mode::Count => slot.positive = slot.positive.saturating_add(hits),
    }
}

fn finalize_files(
    accum: HashMap<PathBuf, HashMap<u32, LineAccum>>,
) -> HashMap<PathBuf, FileCoverage> {
    let mut out = HashMap::new();
    for (path, lines) in accum {
        let mut file = FileCoverage::default();
        for (line_no, slot) in lines {
            let hits = if slot.seen_zero { 0 } else { slot.positive };
            file.lines.insert(line_no, hits);
        }
        out.insert(path, file);
    }
    out
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

/// Rejects spans that would expand into an absurd number of line slots.
///
/// Coverprofile blocks are inclusive `start..=end`. Without a cap, a fuzzed
/// `end` near `u32::MAX` OOMs by inserting billions of map entries.
const MAX_COVER_SPAN_LINES: u32 = 1_000_000;

const fn valid_span(start: u32, end: u32) -> bool {
    start != 0 && end != 0 && end >= start && end.saturating_sub(start) < MAX_COVER_SPAN_LINES
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
