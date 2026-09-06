//! Discover Cargo workspace members via `cargo metadata`.

use crate::error::{Error, Result};
use serde_json::Value;
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

/// Loads every workspace member from `cargo metadata` at `root`.
///
/// # Errors
///
/// Returns [`Error::Metadata`] if Cargo fails or the JSON is incomplete.
pub fn all_members(root: &Path) -> Result<Vec<Package>> {
    let json = run_metadata(root)?;
    packages_from_metadata(&json)
}

/// Loads the named members from the workspace at `root`.
///
/// # Errors
///
/// Returns [`Error::UnknownPackage`] if a name is missing, or
/// [`Error::Metadata`] if Cargo fails.
pub fn selected_members(names: &[String], root: &Path) -> Result<Vec<Package>> {
    let packages = all_members(root)?;
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

fn manifest_path(root: &Path) -> PathBuf {
    root.join("Cargo.toml")
}

fn run_metadata(root: &Path) -> Result<Value> {
    let cargo = std::env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from);
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .arg("--manifest-path")
        .arg(manifest_path(root))
        .output()
        .map_err(|source| Error::Metadata(source.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Metadata(stderr.trim().to_owned()));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| Error::Metadata("metadata was not UTF-8".into()))?;
    serde_json::from_str(&text).map_err(|err| Error::Metadata(err.to_string()))
}

fn packages_from_metadata(json: &Value) -> Result<Vec<Package>> {
    let members = string_ids(json, "workspace_members")?;
    let Some(array) = json.get("packages").and_then(Value::as_array) else {
        return Err(Error::Metadata("missing packages array".into()));
    };
    let mut out = Vec::new();
    for item in array {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !members.iter().any(|m| m == id) {
            continue;
        }
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(manifest) = item.get("manifest_path").and_then(Value::as_str) else {
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

fn string_ids(json: &Value, key: &str) -> Result<Vec<String>> {
    let Some(array) = json.get(key).and_then(Value::as_array) else {
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
        let json = serde_json::from_str(
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
        let value = json.unwrap_or(Value::Null);
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

    #[test]
    fn manifest_path_joins_cargo_toml() {
        assert_eq!(
            manifest_path(Path::new("/other/ws")),
            PathBuf::from("/other/ws/Cargo.toml")
        );
        assert_eq!(manifest_path(Path::new(".")), PathBuf::from("./Cargo.toml"));
    }
}
