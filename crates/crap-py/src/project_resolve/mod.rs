//! Resolve Python projects from `pyproject.toml` and the filesystem.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
mod glob_expand;

use crap_core::{Error, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// A project directory discovered from `pyproject.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    /// Project name used as the report package key.
    pub name: String,
    /// Directory that contains the project's sources.
    pub root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct PyProject {
    project: Option<ProjectTable>,
    tool: Option<ToolTable>,
}

#[derive(Debug, Deserialize)]
struct ProjectTable {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolTable {
    uv: Option<UvTable>,
    poetry: Option<PoetryTable>,
}

#[derive(Debug, Deserialize)]
struct UvTable {
    workspace: Option<UvWorkspace>,
}

#[derive(Debug, Deserialize)]
struct UvWorkspace {
    members: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct PoetryTable {
    name: Option<String>,
}

/// Loads every project under the workspace or single project at `root`.
///
/// When `pyproject.toml` lists `[tool.uv.workspace].members`, expands those
/// globs. Otherwise returns the single project at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if manifests are missing or malformed, or
/// [`Error::Io`] if directories cannot be read.
pub fn all_packages(root: &Path) -> Result<Vec<Package>> {
    let manifest = read_pyproject(root)?;
    let patterns = workspace_patterns(&manifest);
    if patterns.is_empty() {
        return Ok(vec![package_from_manifest(root, &manifest)]);
    }
    discover_workspace_packages(root, &patterns)
}

/// Loads only the project defined by `pyproject.toml` at `root`.
///
/// # Errors
///
/// Returns [`Error::Io`] or [`Error::Resolve`] when `pyproject.toml` cannot be
/// read.
pub fn root_package(root: &Path) -> Result<Package> {
    let manifest = read_pyproject(root)?;
    Ok(package_from_manifest(root, &manifest))
}

/// Loads the named projects under the workspace at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if a name is missing or `pyproject.toml` is
/// invalid.
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

/// Nested project roots under `root` that a walk must not enter.
#[must_use]
pub fn nested_package_roots(root: &Path, all: &[Package]) -> Vec<PathBuf> {
    all.iter()
        .map(|pkg| pkg.root.as_path())
        .filter(|other| *other != root && other.starts_with(root))
        .map(Path::to_path_buf)
        .collect()
}

fn read_pyproject(root: &Path) -> Result<PyProject> {
    let path = root.join("pyproject.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) => return Err(Error::io(&path, source)),
    };
    match toml::from_str(&text) {
        Ok(manifest) => Ok(manifest),
        Err(err) => Err(Error::resolve(format!(
            "{}: invalid pyproject.toml: {err}",
            path.display()
        ))),
    }
}

fn package_from_manifest(root: &Path, manifest: &PyProject) -> Package {
    Package {
        name: project_name(manifest).unwrap_or_else(|| fallback_name(root)),
        root: root.to_path_buf(),
    }
}

fn project_name(manifest: &PyProject) -> Option<String> {
    if let Some(name) = manifest.project.as_ref().and_then(|p| p.name.clone()) {
        return Some(name);
    }
    manifest
        .tool
        .as_ref()
        .and_then(|t| t.poetry.as_ref())
        .and_then(|p| p.name.clone())
}

fn fallback_name(root: &Path) -> String {
    root.file_name().and_then(|n| n.to_str()).unwrap_or("package").to_owned()
}

fn workspace_patterns(manifest: &PyProject) -> Vec<String> {
    manifest
        .tool
        .as_ref()
        .and_then(|t| t.uv.as_ref())
        .and_then(|u| u.workspace.as_ref())
        .and_then(|w| w.members.clone())
        .unwrap_or_default()
}

fn discover_workspace_packages(root: &Path, patterns: &[String]) -> Result<Vec<Package>> {
    let dirs = glob_expand::matching_package_dirs(root, patterns)?;
    let mut packages = Vec::with_capacity(dirs.len());
    for dir in dirs {
        packages.push(load_package(&dir)?);
    }
    if packages.is_empty() {
        return Err(Error::resolve(format!(
            "{}: workspace patterns matched no packages",
            root.display()
        )));
    }
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

fn load_package(path: &Path) -> Result<Package> {
    let manifest = read_pyproject(path)?;
    Ok(package_from_manifest(path, &manifest))
}

#[cfg(test)]
#[path = "../project_resolve_tests.rs"]
mod tests;
