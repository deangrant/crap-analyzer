//! Workspace metadata and feature-graph tests.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
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

fn empty_pkg(name: &str, root: &str) -> Package {
    Package {
        name: name.into(),
        root: PathBuf::from(root),
        features: BTreeMap::new(),
    }
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
    assert!(pkgs[0].features.is_empty());
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
              "std": ["serde"],
              "serde": []
            }
          }]
        }"#,
    );
    assert_eq!(
        pkgs[0].features.get("default"),
        Some(&vec!["std".to_owned()])
    );
    assert_eq!(pkgs[0].features.get("std"), Some(&vec!["serde".to_owned()]));
    assert!(!pkgs[0].features.contains_key("not-a-feature"));
}

#[test]
fn close_features_walks_the_default_graph() {
    let mut map = BTreeMap::new();
    map.insert("default".into(), vec!["std".into()]);
    map.insert("std".into(), vec!["serde".into()]);
    map.insert("serde".into(), Vec::new());
    map.insert(
        "extra".into(),
        vec!["dep:extra".into(), "other/feat".into()],
    );
    let closed = close_features(&map, &["std".into()]);
    assert_eq!(closed, vec!["serde".to_owned(), "std".to_owned()]);
    assert!(close_features(&map, &["extra".into()]) == vec!["extra".to_owned()]);
}

#[test]
fn close_features_handles_cycles_and_unknown_seeds() {
    let mut map = BTreeMap::new();
    map.insert("a".into(), vec!["b".into()]);
    map.insert("b".into(), vec!["a".into()]);
    assert_eq!(
        close_features(&map, &["a".into()]),
        vec!["a".to_owned(), "b".to_owned()]
    );
    assert_eq!(
        close_features(&map, &["orphan".into()]),
        vec!["orphan".to_owned()]
    );
}

#[test]
fn nested_roots_are_children_only() {
    let all = vec![
        empty_pkg("root", "/ws"),
        empty_pkg("inner", "/ws/inner"),
        empty_pkg("sib", "/other"),
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
fn workspace_from_metadata_rejects_bad_packages() {
    let json = serde_json::json!({
        "workspace_root": "/tmp",
        "workspace_members": ["pkg missing 1"],
        "packages": []
    });
    assert!(workspace_from_metadata(&json).is_err());
}

#[test]
fn cargo_metadata_spawn_failure_is_resolve_error() {
    let err = run_cargo_metadata(Path::new("/no/such/crap-rs-cargo"), Path::new("."));
    assert!(err.is_err());
    let message = err.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("cargo metadata"), "{message}");
}

#[test]
fn cargo_metadata_non_utf8_stdout_is_resolve_error() {
    let dir = std::env::temp_dir().join(format!(
        "crap-rs-lib-fake-cargo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let created = std::fs::create_dir_all(&dir);
    assert!(created.is_ok(), "{created:?}");
    let cargo = dir.join("fake-cargo");
    let written = std::fs::write(&cargo, "#!/bin/sh\n/usr/bin/printf '\\xff'\nexit 0\n");
    assert!(written.is_ok(), "{written:?}");
    let mode = std::process::Command::new("chmod").arg("+x").arg(&cargo).status();
    assert!(mode.is_ok(), "{mode:?}");
    let err = run_cargo_metadata(&cargo, Path::new("."));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.is_err());
    let message = err.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("UTF-8"), "{message}");
}

#[test]
fn missing_workspace_root_is_metadata_error() {
    let json = serde_json::json!({
        "workspace_members": ["pkg a 1"],
        "packages": [{
            "id": "pkg a 1",
            "name": "a",
            "manifest_path": "/tmp/a/Cargo.toml"
        }]
    });
    let ws = workspace_from_metadata(&json);
    assert!(ws.is_err());
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
fn packages_without_required_fields_are_an_error() {
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
    assert!(pkgs.is_err());
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
fn packages_for_path_selects_the_member_root() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let selected = packages_for_path(&root);
    assert!(selected.is_ok(), "{selected:?}");
    let selected = selected.unwrap_or_default();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "crap-rs");
}

#[test]
fn packages_for_path_rejects_a_non_member_dir() {
    let workspace = Workspace {
        root: PathBuf::from("/no/such/crap-rs-ws"),
        packages: vec![empty_pkg("a", "/no/such/crap-rs-ws/a")],
    };
    let selected = packages_in_workspace(Path::new("/no/such/crap-rs-ws/src"), workspace);
    assert!(selected.is_err(), "{selected:?}");
    let err = selected.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        err.contains("not the workspace root or a package root"),
        "{err}"
    );
}

#[test]
fn same_path_compares_verbatim_when_canonicalize_fails() {
    let missing = Path::new("/no/such/crap-rs-same-path");
    assert!(same_path(missing, missing));
    assert!(!same_path(missing, Path::new("/no/such/crap-rs-other")));
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

#[test]
fn duplicate_package_names_are_resolve_error() {
    let pkgs = [empty_pkg("dup", "/tmp/a"), empty_pkg("dup", "/tmp/b")];
    let err = ensure_unique_names(&pkgs);
    assert!(err.is_err(), "{err:?}");
    assert!(format!("{err:?}").contains("duplicate package name"));
}
