//! Discover Cargo workspace members via `cargo metadata`.

use crate::error::{Error, Result};
use crate::json::Json;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A workspace package that can be walked for Rust sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Cargo package name.
    pub name: String,
    /// Directory that contains this package's `Cargo.toml`.
    pub root: PathBuf,
}

/// Loads every workspace member from `cargo metadata`.
///
/// # Errors
///
/// Returns [`Error::Metadata`] if Cargo fails or the JSON is incomplete.
pub fn all_members() -> Result<Vec<Package>> {
    let json = run_metadata()?;
    packages_from_metadata(&json)
}

/// Loads the named members. Unknown names fail before analysis.
///
/// # Errors
///
/// Returns [`Error::UnknownPackage`] if a name is missing, or
/// [`Error::Metadata`] if Cargo fails.
pub fn selected_members(names: &[String]) -> Result<Vec<Package>> {
    let packages = all_members()?;
    let mut selected = Vec::with_capacity(names.len());
    for name in names {
        let Some(pkg) = packages.iter().find(|p| p.name == *name) else {
            return Err(Error::UnknownPackage(name.clone()));
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

fn run_metadata() -> Result<Json> {
    let cargo = std::env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from);
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|source| Error::Metadata(source.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Metadata(stderr.trim().to_owned()));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| Error::Metadata("metadata was not UTF-8".into()))?;
    Json::parse(&text).map_err(Error::Metadata)
}

fn packages_from_metadata(json: &Json) -> Result<Vec<Package>> {
    let members = string_ids(json, "workspace_members")?;
    let Some(array) = json.get("packages").and_then(Json::as_array) else {
        return Err(Error::Metadata("missing packages array".into()));
    };
    let mut out = Vec::new();
    for item in array {
        let Some(id) = item.get("id").and_then(Json::as_str) else {
            continue;
        };
        if !members.iter().any(|m| m == id) {
            continue;
        }
        let Some(name) = item.get("name").and_then(Json::as_str) else {
            continue;
        };
        let Some(manifest) = item.get("manifest_path").and_then(Json::as_str) else {
            continue;
        };
        let root = Path::new(manifest)
            .parent()
            .ok_or_else(|| Error::Metadata("manifest_path has no parent".into()))?
            .to_path_buf();
        out.push(Package {
            name: name.to_owned(),
            root,
        });
    }
    Ok(out)
}

fn string_ids(json: &Json, key: &str) -> Result<Vec<String>> {
    let Some(array) = json.get(key).and_then(Json::as_array) else {
        return Err(Error::Metadata(format!("missing {key} array")));
    };
    let mut ids = Vec::new();
    for item in array {
        match item.as_str() {
            Some(id) => ids.push(id.to_owned()),
            None => return Err(Error::Metadata(format!("{key} must be strings"))),
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_members_from_metadata_json() {
        let json = Json::parse(
            r#"{
              "workspace_members": ["pkg a 1"],
              "packages": [
                {
                  "name": "a",
                  "id": "pkg a 1",
                  "manifest_path": "/tmp/a/Cargo.toml"
                },
                {
                  "name": "b",
                  "id": "pkg b 1",
                  "manifest_path": "/tmp/b/Cargo.toml"
                }
              ]
            }"#,
        );
        assert!(json.is_ok());
        let value = json.unwrap_or(Json::Null);
        let pkgs = packages_from_metadata(&value);
        assert!(pkgs.is_ok());
        let pkgs = pkgs.unwrap_or_default();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "a");
        assert_eq!(pkgs[0].root, PathBuf::from("/tmp/a"));
    }

    #[test]
    fn nested_roots_are_children_only() {
        let all = vec![
            Package {
                name: "root".into(),
                root: PathBuf::from("/ws"),
            },
            Package {
                name: "inner".into(),
                root: PathBuf::from("/ws/inner"),
            },
            Package {
                name: "sib".into(),
                root: PathBuf::from("/other"),
            },
        ];
        let skip = nested_member_roots(Path::new("/ws"), &all);
        assert_eq!(skip, vec![PathBuf::from("/ws/inner")]);
    }
}
