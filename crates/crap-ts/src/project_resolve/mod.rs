//! Resolve npm packages from `package.json` and the filesystem.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
mod glob_expand;
mod jsonc;
mod pnpm;

use crap_core::{Error, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// A package directory discovered from `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Package `name` used as the report package key.
    pub name: String,
    /// Directory that contains the package's sources.
    pub root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct PackageJson {
    name: Option<String>,
    workspaces: Option<WorkspacesField>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WorkspacesField {
    List(Vec<String>),
    Object { packages: Option<Vec<String>> },
}

/// Loads every package under the workspace or single package at `root`.
///
/// When `package.json` lists `workspaces` (or `pnpm-workspace.yaml` lists
/// `packages`), expands those globs. Otherwise returns the single package at
/// `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if manifests are missing or malformed, or
/// [`Error::Io`] if directories cannot be read.
pub fn all_packages(root: &Path) -> Result<Vec<Package>> {
    let packages = packages_for_root(root)?;
    ensure_unique_names(&packages)?;
    Ok(packages)
}

fn packages_for_root(root: &Path) -> Result<Vec<Package>> {
    let manifest = read_package_json(root)?;
    let patterns = workspace_patterns(root, &manifest)?;
    if patterns.is_empty() {
        Ok(vec![package_from_manifest(root, &manifest)])
    } else {
        discover_workspace_packages(root, &patterns)
    }
}

/// Loads only the package defined by `package.json` at `root` (no workspace expand).
///
/// # Errors
///
/// Returns [`Error::Io`] or [`Error::Resolve`] when `package.json` cannot be read.
pub fn root_package(root: &Path) -> Result<Package> {
    let manifest = read_package_json(root)?;
    Ok(package_from_manifest(root, &manifest))
}

/// Loads the named packages under the workspace at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if a name is missing or `package.json` is invalid.
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

/// Nested package roots under `root` that a walk must not enter.
#[must_use]
pub fn nested_package_roots(root: &Path, all: &[Package]) -> Vec<PathBuf> {
    all.iter()
        .map(|pkg| pkg.root.as_path())
        .filter(|other| *other != root && other.starts_with(root))
        .map(Path::to_path_buf)
        .collect()
}

fn read_package_json(root: &Path) -> Result<PackageJson> {
    let path = root.join("package.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) => return Err(Error::io(&path, source)),
    };
    let stripped = jsonc::strip_jsonc(&text);
    match serde_json::from_str(&stripped) {
        Ok(manifest) => Ok(manifest),
        Err(err) => Err(Error::resolve(format!(
            "{}: invalid package.json: {err}",
            path.display()
        ))),
    }
}

fn ensure_unique_names(packages: &[Package]) -> Result<()> {
    for (index, pkg) in packages.iter().enumerate() {
        if let Some(other) = packages[..index].iter().find(|p| p.name == pkg.name) {
            return Err(Error::resolve(format!(
                "duplicate package name `{}` at {} and {}",
                pkg.name,
                other.root.display(),
                pkg.root.display()
            )));
        }
    }
    Ok(())
}

fn package_from_manifest(root: &Path, manifest: &PackageJson) -> Package {
    Package {
        name: manifest.name.as_ref().map_or_else(|| fallback_name(root), Clone::clone),
        root: root.to_path_buf(),
    }
}

fn fallback_name(root: &Path) -> String {
    root.file_name().and_then(|n| n.to_str()).unwrap_or("package").to_owned()
}

fn workspace_patterns(root: &Path, manifest: &PackageJson) -> Result<Vec<String>> {
    if let Some(patterns) = patterns_from_workspaces(manifest.workspaces.as_ref()) {
        return Ok(patterns);
    }
    Ok(pnpm::pnpm_workspace_patterns(root)?.unwrap_or_default())
}

fn patterns_from_workspaces(workspaces: Option<&WorkspacesField>) -> Option<Vec<String>> {
    match workspaces? {
        WorkspacesField::List(patterns)
        | WorkspacesField::Object {
            packages: Some(patterns),
        } => Some(patterns.clone()),
        WorkspacesField::Object { packages: None } => Some(Vec::new()),
    }
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
    let manifest = read_package_json(path)?;
    Ok(package_from_manifest(path, &manifest))
}

#[cfg(test)]
#[path = "../project_resolve_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../project_resolve_glob_tests.rs"]
mod glob_tests;
