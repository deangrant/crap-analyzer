//! Discover Cargo workspace members via `cargo metadata`.

use crap_core::{Error, Result};
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
    /// Names listed in the package `default` feature.
    pub default_features: Vec<String>,
    /// Named features other than `default`.
    pub all_features: Vec<String>,
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
    let array = json
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Metadata("missing packages array".into()))?;
    let mut out = Vec::new();
    for item in array {
        if let Some(pkg) = package_from_item(item, &members)? {
            out.push(pkg);
        }
    }
    Ok(out)
}

fn field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key).and_then(Value::as_str)
}

fn package_from_item(item: &Value, members: &[String]) -> Result<Option<Package>> {
    let Some(id) = field(item, "id") else {
        return Ok(None);
    };
    if !members.iter().any(|m| m == id) {
        return Ok(None);
    }
    let Some(name) = field(item, "name") else {
        return Ok(None);
    };
    let Some(manifest) = field(item, "manifest_path") else {
        return Ok(None);
    };
    let root = Path::new(manifest)
        .parent()
        .ok_or_else(|| Error::Metadata("manifest_path has no parent".into()))?
        .to_path_buf();
    let (default_features, all_features) = features_from_package(item);
    Ok(Some(Package {
        name: name.to_owned(),
        root,
        default_features,
        all_features,
    }))
}

fn features_from_package(item: &Value) -> (Vec<String>, Vec<String>) {
    let Some(obj) = item.get("features").and_then(Value::as_object) else {
        return (Vec::new(), Vec::new());
    };
    let default_features = obj
        .get("default")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    let all_features = obj.keys().filter(|key| *key != "default").cloned().collect();
    (default_features, all_features)
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
        let value = json.unwrap_or_default();
        let pkgs = packages_from_metadata(&value);
        assert!(pkgs.is_ok());
        let pkgs = pkgs.unwrap_or_default();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "a");
        assert_eq!(pkgs[0].root, PathBuf::from("/tmp/a"));
        assert!(pkgs[0].default_features.is_empty());
        assert!(pkgs[0].all_features.is_empty());
    }

    #[test]
    fn reads_package_features_from_metadata() {
        let json = serde_json::from_str(
            r#"{
              "workspace_members": ["pkg a 1"],
              "packages": [{
                "name": "a",
                "id": "pkg a 1",
                "manifest_path": "/tmp/a/Cargo.toml",
                "features": {
                  "default": ["std"],
                  "std": [],
                  "serde": ["std"]
                }
              }]
            }"#,
        );
        assert!(json.is_ok());
        let pkgs = packages_from_metadata(&json.unwrap_or_default());
        assert!(pkgs.is_ok());
        let pkgs = pkgs.unwrap_or_default();
        assert_eq!(pkgs[0].default_features, vec!["std".to_owned()]);
        assert!(pkgs[0].all_features.contains(&"std".to_owned()));
        assert!(pkgs[0].all_features.contains(&"serde".to_owned()));
        assert!(!pkgs[0].all_features.contains(&"default".to_owned()));
    }

    #[test]
    fn nested_roots_are_children_only() {
        let all = vec![
            Package {
                name: "root".into(),
                root: PathBuf::from("/ws"),
                default_features: Vec::new(),
                all_features: Vec::new(),
            },
            Package {
                name: "inner".into(),
                root: PathBuf::from("/ws/inner"),
                default_features: Vec::new(),
                all_features: Vec::new(),
            },
            Package {
                name: "sib".into(),
                root: PathBuf::from("/other"),
                default_features: Vec::new(),
                all_features: Vec::new(),
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

    #[test]
    fn missing_packages_array_is_metadata_error() {
        let json = serde_json::json!({ "workspace_members": ["pkg a 1"] });
        let pkgs = packages_from_metadata(&json);
        assert!(pkgs.is_err());
    }

    #[test]
    fn missing_workspace_members_is_metadata_error() {
        let json = serde_json::json!({ "packages": [] });
        let pkgs = packages_from_metadata(&json);
        assert!(pkgs.is_err());
    }

    #[test]
    fn non_string_workspace_members_are_rejected() {
        let json = serde_json::json!({
            "workspace_members": [1],
            "packages": []
        });
        let pkgs = packages_from_metadata(&json);
        assert!(pkgs.is_err());
    }

    #[test]
    fn packages_without_required_fields_are_skipped() {
        let json = serde_json::json!({
            "workspace_members": ["keep 1", "noname 1", "noman 1"],
            "packages": [
                { "name": "noid", "manifest_path": "/tmp/x/Cargo.toml" },
                { "id": "noname 1", "manifest_path": "/tmp/m/Cargo.toml" },
                { "id": "noman 1", "name": "noman" },
                { "id": "keep 1", "name": "keep", "manifest_path": "/tmp/k/Cargo.toml" }
            ]
        });
        let pkgs = packages_from_metadata(&json);
        assert!(pkgs.is_ok());
        let pkgs = pkgs.unwrap_or_default();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "keep");
    }

    #[test]
    fn manifest_without_parent_is_metadata_error() {
        let json = serde_json::json!({
            "workspace_members": ["root 1"],
            "packages": [
                { "id": "root 1", "name": "root", "manifest_path": "/" }
            ]
        });
        let pkgs = packages_from_metadata(&json);
        assert!(pkgs.is_err());
    }

    #[test]
    fn selected_members_returns_named_package() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = root.parent().and_then(|p| p.parent()).unwrap_or_else(|| Path::new("."));
        let selected = selected_members(&["crap-rs".into()], workspace);
        assert!(selected.is_ok(), "{selected:?}");
        let selected = selected.unwrap_or_default();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "crap-rs");
    }

    #[test]
    fn metadata_fails_without_manifest() {
        let dir = std::env::temp_dir().join(format!(
            "crap-rs-nometa-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let created = std::fs::create_dir_all(&dir);
        assert!(created.is_ok(), "{created:?}");
        let result = all_members(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err());
    }
}
