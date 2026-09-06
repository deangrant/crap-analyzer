//! Discover Cargo workspace members via `cargo metadata`.

use crap_core::{Error, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A workspace package that can be walked for Rust sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Cargo package name.
    pub name: String,
    /// Directory that contains this package's `Cargo.toml`.
    pub root: PathBuf,
    /// Feature name to the features it enables (Cargo metadata `features`).
    pub features: BTreeMap<String, Vec<String>>,
}

/// Workspace root and member packages from `cargo metadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// Absolute workspace root from metadata.
    pub root: PathBuf,
    /// Resolved workspace members.
    pub packages: Vec<Package>,
}

/// Loads every workspace member from `cargo metadata` at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if Cargo fails or the JSON is incomplete.
pub fn all_members(root: &Path) -> Result<Vec<Package>> {
    Ok(load(root)?.packages)
}

/// Loads workspace metadata at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if Cargo fails or the JSON is incomplete.
pub fn load(root: &Path) -> Result<Workspace> {
    workspace_from_metadata(&run_metadata(root)?)
}

/// Members to walk for `--path` when a `Cargo.toml` is present.
///
/// The workspace root yields every member. A package root yields that package.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if Cargo fails, JSON is incomplete, or `path` is
/// neither the workspace root nor a member package root.
pub fn packages_for_path(path: &Path) -> Result<Vec<Package>> {
    packages_in_workspace(path, load(path)?)
}

fn packages_in_workspace(path: &Path, workspace: Workspace) -> Result<Vec<Package>> {
    if same_path(path, &workspace.root) {
        return Ok(workspace.packages);
    }
    let here: Vec<Package> = workspace
        .packages
        .into_iter()
        .filter(|pkg| same_path(path, &pkg.root))
        .collect();
    if here.is_empty() {
        return Err(Error::resolve(format!(
            "cargo metadata: {} is not the workspace root or a package root",
            path.display()
        )));
    }
    Ok(here)
}

/// Loads the named members from the workspace at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if a name is missing or Cargo fails.
pub fn selected_members(names: &[String], root: &Path) -> Result<Vec<Package>> {
    let packages = all_members(root)?;
    let mut selected = Vec::with_capacity(names.len());
    for name in names {
        let Some(pkg) = packages.iter().find(|p| p.name == *name) else {
            return Err(Error::resolve(format!("unknown package `{name}`")));
        };
        selected.push(pkg.clone());
    }
    Ok(selected)
}

/// Other member roots nested under `root` that a walk must not enter.
#[must_use]
pub fn nested_member_roots(root: &Path, all: &[Package]) -> Vec<PathBuf> {
    all.iter()
        .map(|pkg| pkg.root.as_path())
        .filter(|other| *other != root && other.starts_with(root))
        .map(Path::to_path_buf)
        .collect()
}

/// Transitively enabled `cfg(feature)` names from `seeds` through `map`.
#[must_use]
pub fn close_features(map: &BTreeMap<String, Vec<String>>, seeds: &[String]) -> Vec<String> {
    let mut enabled = Vec::new();
    let mut stack: Vec<String> =
        seeds.iter().filter(|name| is_cfg_feature(name)).cloned().collect();
    while let Some(name) = stack.pop() {
        if enabled.iter().any(|seen| seen == &name) {
            continue;
        }
        enabled.push(name.clone());
        if let Some(deps) = map.get(&name) {
            stack.extend(deps.iter().filter(|dep| is_cfg_feature(dep)).cloned());
        }
    }
    enabled.sort();
    enabled.dedup();
    enabled
}

fn is_cfg_feature(name: &str) -> bool {
    !name.starts_with("dep:") && !name.contains('/')
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join("Cargo.toml")
}

fn cargo_from_override(value: Option<&Path>) -> PathBuf {
    value
        .filter(|path| path.is_file())
        .map_or_else(|| PathBuf::from("cargo"), Path::to_path_buf)
}

fn cargo_bin() -> PathBuf {
    cargo_from_override(std::env::var_os("CARGO").as_deref().map(Path::new))
}

fn run_metadata(root: &Path) -> Result<Value> {
    run_cargo_metadata(&cargo_bin(), root)
}

fn run_cargo_metadata(cargo: &Path, root: &Path) -> Result<Value> {
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .arg("--manifest-path")
        .arg(manifest_path(root))
        .output()
        .map_err(|source| Error::resolve(format!("cargo metadata: {source}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::resolve(format!("cargo metadata: {}", stderr.trim())));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| Error::resolve("cargo metadata: metadata was not UTF-8"))?;
    serde_json::from_str(&text).map_err(|err| Error::resolve(format!("cargo metadata: {err}")))
}

fn workspace_from_metadata(json: &Value) -> Result<Workspace> {
    let root = json
        .get("workspace_root")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::resolve("cargo metadata: missing workspace_root"))?;
    Ok(Workspace {
        root: PathBuf::from(root),
        packages: packages_from_metadata(json)?,
    })
}

fn packages_from_metadata(json: &Value) -> Result<Vec<Package>> {
    let members = string_ids(json, "workspace_members")?;
    let (out, matched) = collect_packages(packages_array(json)?, &members)?;
    require_matched_members(&members, &matched)?;
    Ok(out)
}

fn collect_packages(array: &[Value], members: &[String]) -> Result<(Vec<Package>, Vec<String>)> {
    let mut out = Vec::new();
    let mut matched = Vec::new();
    for item in array {
        if let Some((pkg, id)) = package_from_item(item, members)? {
            matched.push(id);
            out.push(pkg);
        }
    }
    Ok((out, matched))
}

fn packages_array(json: &Value) -> Result<&Vec<Value>> {
    json.get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::resolve("cargo metadata: missing packages array"))
}

fn require_matched_members(members: &[String], matched: &[String]) -> Result<()> {
    let unmatched: Vec<&String> =
        members.iter().filter(|id| !matched.iter().any(|hit| hit == *id)).collect();
    if unmatched.is_empty() {
        return Ok(());
    }
    Err(Error::resolve(format!(
        "cargo metadata: workspace members did not match packages: {}",
        unmatched.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(", ")
    )))
}

fn field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key).and_then(Value::as_str)
}

fn package_from_item(item: &Value, members: &[String]) -> Result<Option<(Package, String)>> {
    let Some((id, name, manifest)) = member_manifest(item, members) else {
        return Ok(None);
    };
    let root = Path::new(manifest)
        .parent()
        .ok_or_else(|| Error::resolve("cargo metadata: manifest_path has no parent"))?
        .to_path_buf();
    Ok(Some((
        Package {
            name: name.to_owned(),
            root,
            features: features_from_package(item),
        },
        id.to_owned(),
    )))
}

fn member_manifest<'a>(item: &'a Value, members: &[String]) -> Option<(&'a str, &'a str, &'a str)> {
    let id = field(item, "id")?;
    if !members.iter().any(|member| member == id) {
        return None;
    }
    Some((id, field(item, "name")?, field(item, "manifest_path")?))
}

fn features_from_package(item: &Value) -> BTreeMap<String, Vec<String>> {
    let Some(obj) = item.get("features").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    obj.iter()
        .map(|(key, value)| {
            let deps = value
                .as_array()
                .map(|arr| arr.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default();
            (key.clone(), deps)
        })
        .collect()
}

fn string_ids(json: &Value, key: &str) -> Result<Vec<String>> {
    let Some(array) = json.get(key).and_then(Value::as_array) else {
        return Err(Error::resolve(format!(
            "cargo metadata: missing {key} array"
        )));
    };
    let mut ids = Vec::new();
    for item in array {
        match item.as_str() {
            Some(id) => ids.push(id.to_owned()),
            None => {
                return Err(Error::resolve(format!(
                    "cargo metadata: {key} must be strings"
                )));
            }
        }
    }
    Ok(ids)
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
