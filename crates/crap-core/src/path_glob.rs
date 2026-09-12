//! Language-agnostic workspace glob expansion (`*`, `**`, `!` exclusions).

use crate::{Error, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Collect directories matching `patterns` under `root`.
///
/// `is_package` decides whether a directory is a package root.
/// `is_skipped` skips directory names during walks (e.g. `node_modules`, `.venv`).
///
/// # Errors
///
/// Returns [`Error::Io`] when a directory cannot be read.
pub fn matching_package_dirs(
    root: &Path,
    patterns: &[String],
    is_package: impl Fn(&Path) -> bool,
    is_skipped: impl Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>> {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    split_patterns(patterns, &mut includes, &mut excludes);
    let mut found = BTreeMap::new();
    for pattern in &includes {
        expand_include(root, pattern, &mut found, &is_package, &is_skipped)?;
    }
    Ok(found.into_values().filter(|path| !is_excluded(root, path, &excludes)).collect())
}

fn split_patterns(patterns: &[String], includes: &mut Vec<String>, excludes: &mut Vec<String>) {
    for pattern in patterns {
        let normalized = normalize_pattern(pattern);
        if let Some(rest) = normalized.strip_prefix('!') {
            if !rest.is_empty() {
                excludes.push(rest.to_owned());
            }
        } else if !normalized.is_empty() {
            includes.push(normalized);
        }
    }
}

/// Normalize a workspace glob (`./` strip, `!` keep, trim slashes).
#[must_use]
pub fn normalize_pattern(pattern: &str) -> String {
    let trimmed = pattern.trim();
    let (negated, body) = trimmed.strip_prefix('!').map_or((false, trimmed), |rest| (true, rest));
    let stripped = body.trim_start_matches("./").trim_matches('/');
    if negated {
        format!("!{stripped}")
    } else {
        stripped.to_owned()
    }
}

fn expand_include(
    root: &Path,
    pattern: &str,
    found: &mut BTreeMap<PathBuf, PathBuf>,
    is_package: &impl Fn(&Path) -> bool,
    is_skipped: &impl Fn(&Path) -> bool,
) -> Result<()> {
    let segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    walk_segments(root, &segments, 0, found, is_package, is_skipped)
}

fn walk_segments(
    dir: &Path,
    segments: &[&str],
    index: usize,
    found: &mut BTreeMap<PathBuf, PathBuf>,
    is_package: &impl Fn(&Path) -> bool,
    is_skipped: &impl Fn(&Path) -> bool,
) -> Result<()> {
    if index == segments.len() {
        record_package_dir(dir, found, is_package);
        return Ok(());
    }
    match segments[index] {
        "**" => walk_double_star(dir, segments, index, found, is_package, is_skipped),
        "*" => walk_star(dir, segments, index, found, is_package, is_skipped),
        name => walk_segments(
            &dir.join(name),
            segments,
            index + 1,
            found,
            is_package,
            is_skipped,
        ),
    }
}

fn walk_double_star(
    dir: &Path,
    segments: &[&str],
    index: usize,
    found: &mut BTreeMap<PathBuf, PathBuf>,
    is_package: &impl Fn(&Path) -> bool,
    is_skipped: &impl Fn(&Path) -> bool,
) -> Result<()> {
    walk_segments(dir, segments, index + 1, found, is_package, is_skipped)?;
    walk_double_star_children(dir, segments, index, found, is_package, is_skipped)
}

fn walk_double_star_children(
    dir: &Path,
    segments: &[&str],
    index: usize,
    found: &mut BTreeMap<PathBuf, PathBuf>,
    is_package: &impl Fn(&Path) -> bool,
    is_skipped: &impl Fn(&Path) -> bool,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for_each_child_dir(dir, is_skipped, |path| {
        walk_double_star(path, segments, index, found, is_package, is_skipped)
    })
}

fn walk_star(
    dir: &Path,
    segments: &[&str],
    index: usize,
    found: &mut BTreeMap<PathBuf, PathBuf>,
    is_package: &impl Fn(&Path) -> bool,
    is_skipped: &impl Fn(&Path) -> bool,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for_each_child_dir(dir, is_skipped, |path| {
        walk_segments(path, segments, index + 1, found, is_package, is_skipped)
    })
}

fn for_each_child_dir(
    dir: &Path,
    is_skipped: &impl Fn(&Path) -> bool,
    mut visit: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    let entries = read_dir_entries(dir)?;
    for entry in entries {
        visit_child_entry(dir, entry, is_skipped, &mut visit)?;
    }
    Ok(())
}

fn visit_child_entry(
    dir: &Path,
    entry: std::io::Result<fs::DirEntry>,
    is_skipped: &impl Fn(&Path) -> bool,
    visit: &mut impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    let path = dir_entry_path(dir, entry)?;
    if !path.is_dir() || is_skipped(&path) {
        return Ok(());
    }
    visit(&path)
}

fn record_package_dir(
    dir: &Path,
    found: &mut BTreeMap<PathBuf, PathBuf>,
    is_package: &impl Fn(&Path) -> bool,
) {
    if is_package(dir) {
        found.insert(dir.to_path_buf(), dir.to_path_buf());
    }
}

/// True when `path` matches any exclude glob relative to `root`.
#[must_use]
pub fn is_excluded(root: &Path, path: &Path, excludes: &[String]) -> bool {
    path.strip_prefix(root).ok().is_some_and(|rel| {
        let rel = rel.to_string_lossy().replace('\\', "/");
        excludes.iter().any(|pattern| path_matches_glob(&rel, pattern))
    })
}

/// Match a root-relative path against a glob pattern.
#[must_use]
pub fn path_matches_glob(rel: &str, pattern: &str) -> bool {
    let path_parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    let glob_parts: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    match_segments(&path_parts, &glob_parts)
}

fn match_segments(path: &[&str], pattern: &[&str]) -> bool {
    if pattern.first() == Some(&"**") {
        return match_globstar(path, pattern);
    }
    match_plain(path, pattern)
}

fn match_globstar(path: &[&str], pattern: &[&str]) -> bool {
    match_segments(path, &pattern[1..]) || (!path.is_empty() && match_segments(&path[1..], pattern))
}

fn match_plain(path: &[&str], pattern: &[&str]) -> bool {
    match (path.first(), pattern.first()) {
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
        (Some(seg), Some(pat)) => match_plain_heads(seg, pat, path, pattern),
    }
}

fn match_plain_heads(seg: &str, pat: &str, path: &[&str], pattern: &[&str]) -> bool {
    if pat == "*" || seg == pat {
        return match_segments(&path[1..], &pattern[1..]);
    }
    false
}

fn read_dir_entries(dir: &Path) -> Result<fs::ReadDir> {
    match fs::read_dir(dir) {
        Ok(entries) => Ok(entries),
        Err(source) => Err(Error::io(dir, source)),
    }
}

/// Map a `read_dir` entry to a path, wrapping I/O errors.
///
/// # Errors
///
/// Returns [`Error::Io`] when the entry cannot be read.
pub fn dir_entry_path(dir: &Path, entry: std::io::Result<fs::DirEntry>) -> Result<PathBuf> {
    match entry {
        Ok(entry) => Ok(entry.path()),
        Err(source) => Err(Error::io(dir, source)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dir_entry_path, is_excluded, matching_package_dirs, normalize_pattern, path_matches_glob,
    };
    use std::path::Path;

    #[test]
    fn match_segments_star_and_literal() {
        assert!(path_matches_glob("packages/a", "packages/*"));
        assert!(path_matches_glob("packages/a/b", "packages/**"));
        assert!(!path_matches_glob("packages/a", "apps/*"));
    }

    #[test]
    fn normalize_strips_dot_slash_and_keeps_bang() {
        assert_eq!(normalize_pattern("./packages/*"), "packages/*");
        assert_eq!(normalize_pattern("!./packages/skip"), "!packages/skip");
    }

    #[test]
    fn exclude_ignores_paths_outside_root() {
        assert!(!is_excluded(
            Path::new("/workspace"),
            Path::new("/other/pkg"),
            &["packages/skip".into()],
        ));
    }

    #[test]
    fn empty_and_bang_only_patterns_are_skipped() {
        let root = Path::new("/tmp/crap-core-glob-empty-patterns");
        let got = matching_package_dirs(root, &["!".into(), "   ".into()], |_| false, |_| false);
        assert!(got.is_ok(), "{got:?}");
        assert!(got.unwrap_or_default().is_empty());
    }

    #[test]
    fn dir_entry_path_propagates_io_error() {
        let err = dir_entry_path(Path::new("/tmp"), Err(std::io::Error::other("boom")));
        assert!(err.is_err());
    }
}
