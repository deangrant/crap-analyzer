//! Resolve Go packages from `go.mod` and the filesystem.

use crap_core::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// A Go package directory discovered under a module root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Import path used as the report package name.
    pub name: String,
    /// Directory that contains the package's `.go` files.
    pub root: PathBuf,
}

/// Loads every package under the module at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if `go.mod` is missing or malformed, or
/// [`Error::Io`] if directories cannot be read.
pub fn all_packages(root: &Path) -> Result<Vec<Package>> {
    let module = read_module_path(root)?;
    discover_packages(root, &module)
}

/// Loads the named packages (by import path) under the module at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if a name is missing or `go.mod` is invalid.
pub fn selected_packages(names: &[String], root: &Path) -> Result<Vec<Package>> {
    let packages = all_packages(root)?;
    let mut selected = Vec::with_capacity(names.len());
    for name in names {
        let Some(pkg) = packages.iter().find(|p| p.name == *name) else {
            return Err(Error::resolve(format!("unknown package `{name}`")));
        };
        selected.push(pkg.clone());
    }
    Ok(selected)
}

/// Nested module roots under `root` that a walk must not enter.
#[must_use]
pub fn nested_module_roots(root: &Path, all: &[Package]) -> Vec<PathBuf> {
    all.iter()
        .map(|pkg| pkg.root.as_path())
        .filter(|other| *other != root && other.starts_with(root))
        .map(Path::to_path_buf)
        .collect()
}

/// Walks up from `start` looking for a directory that contains `go.mod`.
///
/// Returns the module root and the `module` path from that file.
///
/// # Errors
///
/// Returns [`Error::Io`] or [`Error::Resolve`] when a `go.mod` is found but
/// cannot be read or has no module path. Returns `Ok(None)` when no enclosing
/// module exists.
pub fn enclosing_module(start: &Path) -> Result<Option<(PathBuf, String)>> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("go.mod").is_file() {
            let module = read_module_path(&dir)?;
            return Ok(Some((dir, module)));
        }
        if !dir.pop() {
            return Ok(None);
        }
    }
}

fn discover_packages(module_root: &Path, module_path: &str) -> Result<Vec<Package>> {
    let mut packages = Vec::new();
    collect_packages(module_root, module_root, module_path, &mut packages)?;
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

fn collect_packages(
    dir: &Path,
    module_root: &Path,
    module_path: &str,
    out: &mut Vec<Package>,
) -> Result<()> {
    if is_nested_module(dir, module_root) {
        return Ok(());
    }
    let collected = read_dir_entries(dir)?;
    maybe_push_package(dir, module_root, module_path, &collected, out);
    walk_package_dirs(collected, dir, module_root, module_path, out)
}

fn is_nested_module(dir: &Path, module_root: &Path) -> bool {
    dir != module_root && dir.join("go.mod").is_file()
}

fn maybe_push_package(
    dir: &Path,
    module_root: &Path,
    module_path: &str,
    collected: &[fs::DirEntry],
    out: &mut Vec<Package>,
) {
    if collected.iter().any(|entry| is_go_source_path(&entry.path())) {
        out.push(Package {
            name: import_path(dir, module_root, module_path),
            root: dir.to_path_buf(),
        });
    }
}

fn walk_package_dirs(
    collected: Vec<fs::DirEntry>,
    dir: &Path,
    module_root: &Path,
    module_path: &str,
    out: &mut Vec<Package>,
) -> Result<()> {
    for entry in collected {
        take_package_dir(Ok(entry), dir, module_root, module_path, out)?;
    }
    Ok(())
}

fn read_dir_entries(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) => return Err(Error::io(dir, source)),
    };
    let mut collected = Vec::new();
    for entry in entries {
        push_dir_entry(entry, dir, &mut collected)?;
    }
    Ok(collected)
}

fn push_dir_entry(
    entry: std::io::Result<fs::DirEntry>,
    dir: &Path,
    out: &mut Vec<fs::DirEntry>,
) -> Result<()> {
    match entry {
        Ok(entry) => {
            out.push(entry);
            Ok(())
        }
        Err(source) => Err(Error::io(dir, source)),
    }
}

fn take_package_dir(
    entry: std::io::Result<fs::DirEntry>,
    dir: &Path,
    module_root: &Path,
    module_path: &str,
    out: &mut Vec<Package>,
) -> Result<()> {
    let entry = match entry {
        Ok(entry) => entry,
        Err(source) => return Err(Error::io(dir, source)),
    };
    let path = entry.path();
    if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
        return Ok(());
    }
    if is_skip_dir_name(&path) {
        return Ok(());
    }
    collect_packages(&path, module_root, module_path, out)
}

fn is_go_source_path(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "go")
        && !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_test.go"))
}

fn import_path(dir: &Path, module_root: &Path, module_path: &str) -> String {
    let Ok(rel) = dir.strip_prefix(module_root) else {
        return module_path.to_owned();
    };
    if rel.as_os_str().is_empty() {
        return module_path.to_owned();
    }
    format!("{module_path}/{}", rel.to_string_lossy().replace('\\', "/"))
}

fn is_skip_dir_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| matches!(name, "vendor" | ".git" | "testdata"))
}

fn read_module_path(root: &Path) -> Result<String> {
    let path = root.join("go.mod");
    let text = read_gomod_text(&path)?;
    module_path_from_text(&text, &path)
}

fn read_gomod_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::io(path, source))
}

fn module_path_from_text(text: &str, path: &Path) -> Result<String> {
    for line in text.lines() {
        if let Some(name) = module_line_name(line) {
            return Ok(name);
        }
    }
    Err(Error::resolve(format!(
        "{}: missing module path",
        path.display()
    )))
}

fn module_line_name(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("module")?;
    let name = rest.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
#[path = "module_resolve_tests.rs"]
mod tests;
