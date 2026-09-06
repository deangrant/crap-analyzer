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
/// Returns [`Error::Resolve`] if Cargo fails or the JSON is incomplete.
pub fn all_members(root: &Path) -> Result<Vec<Package>> {
    let json = run_metadata(root)?;
    packages_from_metadata(&json)
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
    let cargo = cargo_bin();
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

fn packages_from_metadata(json: &Value) -> Result<Vec<Package>> {
    let members = string_ids(json, "workspace_members")?;
    let out = collect_packages(packages_array(json)?, &members)?;
    require_matched_members(&members, &out)?;
    Ok(out)
}

fn collect_packages(array: &[Value], members: &[String]) -> Result<Vec<Package>> {
    let mut out = Vec::new();
    for item in array {
        if let Some(pkg) = package_from_item(item, members)? {
            out.push(pkg);
        }
    }
    Ok(out)
}

fn packages_array(json: &Value) -> Result<&Vec<Value>> {
    json.get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::resolve("cargo metadata: missing packages array"))
}

fn require_matched_members(members: &[String], out: &[Package]) -> Result<()> {
    if !members.is_empty() && out.is_empty() {
        return Err(Error::resolve(
            "cargo metadata: workspace members did not match any packages",
        ));
    }
    Ok(())
}

fn field<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key).and_then(Value::as_str)
}

fn package_from_item(item: &Value, members: &[String]) -> Result<Option<Package>> {
    let Some((name, manifest)) = member_manifest(item, members) else {
        return Ok(None);
    };
    let root = Path::new(manifest)
        .parent()
        .ok_or_else(|| Error::resolve("cargo metadata: manifest_path has no parent"))?
        .to_path_buf();
    let (default_features, all_features) = features_from_package(item);
    Ok(Some(Package {
        name: name.to_owned(),
        root,
        default_features,
        all_features,
    }))
}

fn member_manifest<'a>(item: &'a Value, members: &[String]) -> Option<(&'a str, &'a str)> {
    let id = field(item, "id")?;
    if !members.iter().any(|member| member == id) {
        return None;
    }
    Some((field(item, "name")?, field(item, "manifest_path")?))
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
mod tests {
    use super::*;

    fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
        result: std::result::Result<T, E>,
    ) -> T {
        assert!(result.is_ok(), "{result:?}");
        result.unwrap_or_default()
    }

    fn packages_from(text: &str) -> Vec<Package> {
        let value = require_ok(serde_json::from_str(text));
        require_ok(packages_from_metadata(&value))
    }

    #[test]
    fn reads_members_from_metadata_json() {
        let pkgs = packages_from(
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
        assert_eq!(pkgs.len(), 1);
        assert_eq!(
            (pkgs[0].name.as_str(), pkgs[0].root.as_path()),
            ("a", Path::new("/tmp/a"))
        );
        assert!(pkgs[0].default_features.is_empty() && pkgs[0].all_features.is_empty());
    }

    #[test]
    fn reads_package_features_from_metadata() {
        let pkgs = packages_from(
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
        let features = &pkgs[0].all_features;
        assert_eq!(pkgs[0].default_features, vec!["std".to_owned()]);
        assert!(features.contains(&"std".to_owned()) && features.contains(&"serde".to_owned()));
        assert!(!features.contains(&"default".to_owned()));
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
    fn unmatched_workspace_members_is_metadata_error() {
        let json = serde_json::json!({
            "workspace_members": ["pkg missing 1"],
            "packages": [
                {
                    "id": "pkg other 1",
                    "name": "other",
                    "manifest_path": "/tmp/other/Cargo.toml"
                }
            ]
        });
        let pkgs = packages_from_metadata(&json);
        assert!(pkgs.is_err());
        let message = pkgs.as_ref().err().map_or(String::new(), ToString::to_string);
        assert!(message.contains("did not match"), "{message}");
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

    #[test]
    fn cargo_override_requires_an_existing_file() {
        assert_eq!(
            cargo_from_override(Some(Path::new("/no/such/crap-rs-metadata"))),
            PathBuf::from("cargo")
        );
        assert_eq!(cargo_from_override(None), PathBuf::from("cargo"));
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert_eq!(cargo_from_override(Some(&manifest)), manifest);
    }
}
