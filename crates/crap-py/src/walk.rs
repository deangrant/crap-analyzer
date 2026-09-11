//! Collect Python sources under an analysis root.

use crap_core::{Error, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Directories skipped at any depth.
const SKIP_ALWAYS: &[&str] = &[
    ".venv",
    "venv",
    "__pycache__",
    ".git",
    "dist",
    "build",
    ".tox",
    "htmlcov",
    "coverage",
    ".eggs",
];

/// Shared walk state so directory helpers stay under Clippy's argument cap.
struct Walk<'a> {
    root: &'a Path,
    root_canon: &'a Path,
    nested_skip: &'a [PathBuf],
    visited: &'a mut HashSet<PathBuf>,
    out: &'a mut Vec<PathBuf>,
}

/// Walks `root` for `.py` files, skipping nested project roots in `skip`.
///
/// # Errors
///
/// Returns [`Error::Io`] if the root cannot be canonicalized or a directory
/// cannot be read.
pub fn py_files(root: &Path, skip: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let walk_root_canon = fs::canonicalize(root).map_err(|source| Error::io(root, source))?;
    visited.insert(walk_root_canon.clone());
    let mut walk = Walk {
        root,
        root_canon: &walk_root_canon,
        nested_skip: skip,
        visited: &mut visited,
        out: &mut out,
    };
    visit(root, &mut walk)?;
    out.sort();
    Ok(out)
}

fn visit(dir: &Path, walk: &mut Walk<'_>) -> Result<()> {
    if skip_dir(dir, walk.root, walk.nested_skip) {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|source| Error::io(dir, source))?;
    walk_entries(entries, dir, walk)
}

fn walk_entries(
    entries: impl Iterator<Item = std::io::Result<fs::DirEntry>>,
    dir: &Path,
    walk: &mut Walk<'_>,
) -> Result<()> {
    for entry in entries {
        take_entry(entry, dir, walk)?;
    }
    Ok(())
}

fn is_nested_project(path: &Path, walk_root: &Path) -> bool {
    path != walk_root && path.join("pyproject.toml").is_file()
}

fn take_entry(entry: std::io::Result<fs::DirEntry>, dir: &Path, walk: &mut Walk<'_>) -> Result<()> {
    let entry = entry.map_err(|source| Error::io(dir, source))?;
    take_typed_entry(&entry.path(), entry.file_type(), walk)
}

fn take_typed_entry(
    path: &Path,
    file_type: std::io::Result<fs::FileType>,
    walk: &mut Walk<'_>,
) -> Result<()> {
    let file_type = file_type.map_err(|source| Error::io(path, source))?;
    if file_type.is_symlink() {
        return take_symlink(path, walk);
    }
    if file_type.is_dir() {
        return visit_subdir(path, walk);
    }
    collect_py_file(path.to_path_buf(), walk);
    Ok(())
}

fn take_symlink(path: &Path, walk: &mut Walk<'_>) -> Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };
    if meta.is_dir() {
        return visit_subdir(path, walk);
    }
    if meta.is_file() {
        collect_py_file(path.to_path_buf(), walk);
    }
    Ok(())
}

fn visit_subdir(path: &Path, walk: &mut Walk<'_>) -> Result<()> {
    let Ok(canon) = fs::canonicalize(path) else {
        return Ok(());
    };
    if !stays_in_root(&canon, walk.root_canon) {
        return Ok(());
    }
    if !walk.visited.insert(canon) {
        return Ok(());
    }
    visit(path, walk)
}

fn collect_py_file(path: PathBuf, walk: &mut Walk<'_>) {
    if !is_py_source(&path) {
        return;
    }
    let Ok(canon) = fs::canonicalize(&path) else {
        return;
    };
    if !stays_in_root(&canon, walk.root_canon) || !walk.visited.insert(canon) {
        return;
    }
    walk.out.push(path);
}

fn stays_in_root(canon: &Path, walk_root_canon: &Path) -> bool {
    canon.starts_with(walk_root_canon)
}

fn skip_dir(dir: &Path, walk_root: &Path, nested_skip: &[PathBuf]) -> bool {
    if nested_skip.iter().any(|skip| dir == skip) {
        return true;
    }
    if dir == walk_root {
        return false;
    }
    if is_nested_project(dir, walk_root) {
        return true;
    }
    dir.file_name().and_then(|n| n.to_str()).is_some_and(is_skipped_name)
}

fn is_skipped_name(name: &str) -> bool {
    SKIP_ALWAYS.contains(&name) || name.ends_with(".egg-info")
}

fn is_py_source(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("py")
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
