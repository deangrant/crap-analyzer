//! Collect Rust sources under an analysis root.

use crap_core::{Error, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Directories skipped at any depth.
const SKIP_ALWAYS: &[&str] = &["target", ".git"];

/// Cargo convention dirs skipped only as children of a package root.
const CONVENTION_DIRS: &[&str] = &["tests", "benches", "examples"];

/// Shared walk state so directory helpers stay under Clippy's argument cap.
struct Walk<'a> {
    root: &'a Path,
    root_canon: &'a Path,
    nested_skip: &'a [PathBuf],
    visited: &'a mut HashSet<PathBuf>,
    out: &'a mut Vec<PathBuf>,
}

/// Walks `root` for `.rs` files, skipping nested member roots.
///
/// # Errors
///
/// Returns [`Error::Io`] if the root cannot be canonicalized or a directory
/// cannot be read.
pub fn rust_files(root: &Path, nested_skip: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let walk_root_canon = fs::canonicalize(root).map_err(|source| Error::io(root, source))?;
    visited.insert(walk_root_canon.clone());
    let mut walk = Walk {
        root,
        root_canon: &walk_root_canon,
        nested_skip,
        visited: &mut visited,
        out: &mut out,
    };
    visit(root, None, &mut walk)?;
    out.sort();
    Ok(out)
}

fn visit(dir: &Path, inherited_pkg: Option<&Path>, walk: &mut Walk<'_>) -> Result<()> {
    let package_root_buf = dir.join("Cargo.toml").is_file().then(|| dir.to_path_buf());
    let package_root = package_root_buf.as_deref().or(inherited_pkg);
    if skip_dir(dir, walk.root, package_root, walk.nested_skip) {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|source| Error::io(dir, source))?;
    walk_entries(entries, dir, package_root, walk)
}

fn walk_entries(
    entries: impl Iterator<Item = std::io::Result<fs::DirEntry>>,
    dir: &Path,
    package_root: Option<&Path>,
    walk: &mut Walk<'_>,
) -> Result<()> {
    for entry in entries {
        take_entry(entry, dir, package_root, walk)?;
    }
    Ok(())
}

fn take_entry(
    entry: std::io::Result<fs::DirEntry>,
    dir: &Path,
    package_root: Option<&Path>,
    walk: &mut Walk<'_>,
) -> Result<()> {
    let entry = entry.map_err(|source| Error::io(dir, source))?;
    take_typed_entry(&entry.path(), entry.file_type(), package_root, walk)
}

fn take_typed_entry(
    path: &Path,
    file_type: std::io::Result<fs::FileType>,
    package_root: Option<&Path>,
    walk: &mut Walk<'_>,
) -> Result<()> {
    let file_type = file_type.map_err(|source| Error::io(path, source))?;
    if file_type.is_symlink() {
        return take_symlink(path, package_root, walk);
    }
    if file_type.is_dir() {
        return visit_subdir(path, package_root, walk);
    }
    collect_rust_file(path.to_path_buf(), walk);
    Ok(())
}

fn take_symlink(path: &Path, package_root: Option<&Path>, walk: &mut Walk<'_>) -> Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };
    if meta.is_dir() {
        return visit_subdir(path, package_root, walk);
    }
    if meta.is_file() {
        collect_rust_file(path.to_path_buf(), walk);
    }
    Ok(())
}

fn visit_subdir(path: &Path, package_root: Option<&Path>, walk: &mut Walk<'_>) -> Result<()> {
    let Ok(canon) = fs::canonicalize(path) else {
        return Ok(());
    };
    if !stays_in_root(&canon, walk.root_canon) {
        return Ok(());
    }
    if !walk.visited.insert(canon) {
        return Ok(());
    }
    visit(path, package_root, walk)
}

fn collect_rust_file(path: PathBuf, walk: &mut Walk<'_>) {
    if !is_rust_file(&path) {
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

fn skip_dir(
    dir: &Path,
    walk_root: &Path,
    package_root: Option<&Path>,
    nested_skip: &[PathBuf],
) -> bool {
    if nested_skip.iter().any(|skip| dir == skip) {
        return true;
    }
    if dir == walk_root {
        return false;
    }
    skip_named_dir(dir, package_root)
}

fn skip_named_dir(dir: &Path, package_root: Option<&Path>) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if SKIP_ALWAYS.contains(&name) {
        return true;
    }
    package_root.is_some_and(|pkg| dir.parent() == Some(pkg) && CONVENTION_DIRS.contains(&name))
}

fn is_rust_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "rs")
        && !path.file_name().and_then(|n| n.to_str()).is_some_and(is_test_source_name)
}

fn is_test_source_name(name: &str) -> bool {
    name == "tests.rs" || name.ends_with("_tests.rs")
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
