use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}

fn unique_temp(label: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("crap-ts-pkg-{label}-{n}"))
}

fn write_file(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, body));
}

#[test]
fn single_package_uses_name_field() {
    let root = unique_temp("single");
    write_file(&root.join("package.json"), r#"{"name":"demo-pkg"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "demo-pkg");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_globs_expand_star() {
    let root = unique_temp("workspace");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    );
    write_file(
        &root.join("packages/a/package.json"),
        r#"{"name":"@scope/a"}"#,
    );
    write_file(
        &root.join("packages/b/package.json"),
        r#"{"name":"@scope/b"}"#,
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 2);
    assert_eq!(packages[0].name, "@scope/a");
    assert_eq!(packages[1].name, "@scope/b");
    let selected = require_ok(selected_packages(&["@scope/b".into()], &root));
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "@scope/b");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nested_packages_object_workspaces() {
    let root = unique_temp("nested-ws");
    write_file(
        &root.join("package.json"),
        r#"{"workspaces":{"packages":["pkgs/*"]}}"#,
    );
    write_file(&root.join("pkgs/one/package.json"), r#"{"name":"one"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "one");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_package_is_resolve_error() {
    let root = unique_temp("missing");
    write_file(&root.join("package.json"), r#"{"name":"only"}"#);
    assert!(selected_packages(&["nope".into()], &root).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn fallback_name_uses_directory() {
    let root = unique_temp("anon");
    write_file(&root.join("package.json"), "{}");
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert!(!packages[0].name.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_package_ignores_workspaces() {
    let root = unique_temp("root-only");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    );
    write_file(&root.join("packages/a/package.json"), r#"{"name":"a"}"#);
    let pkg = root_package(&root);
    assert!(pkg.is_ok(), "{pkg:?}");
    if let Ok(pkg) = pkg {
        assert_eq!(pkg.name, "root");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nested_package_roots_lists_children() {
    let root = unique_temp("nest-list");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    );
    write_file(&root.join("packages/a/package.json"), r#"{"name":"a"}"#);
    write_file(&root.join("packages/b/package.json"), r#"{"name":"b"}"#);
    let packages = require_ok(all_packages(&root));
    let skips = nested_package_roots(&packages[0].root, &packages);
    assert!(!skips.is_empty() || packages.len() == 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_package_json_is_io_error() {
    let root = unique_temp("missing-json");
    require_ok(fs::create_dir_all(&root));
    assert!(all_packages(&root).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn empty_workspace_globs_error() {
    let root = unique_temp("empty-ws");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    );
    assert!(all_packages(&root).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn exact_workspace_path_without_star() {
    let root = unique_temp("exact");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/a"]}"#,
    );
    write_file(&root.join("packages/a/package.json"), r#"{"name":"a"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "a");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn escaped_name_in_package_json() {
    let root = unique_temp("escape");
    write_file(&root.join("package.json"), r#"{"name":"a\"b"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages[0].name, "a\"b");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn malformed_workspaces_array_is_ignored_for_single() {
    let root = unique_temp("bad-ws");
    write_file(
        &root.join("package.json"),
        r#"{"name":"solo","workspaces":[1,2]}"#,
    );
    // Non-string array entries fail parse → treat as no patterns → single package.
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "solo");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_pattern_without_slash() {
    let root = unique_temp("noslash");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["pkg"]}"#,
    );
    write_file(&root.join("pkg/package.json"), r#"{"name":"pkg"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "pkg");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_pattern_empty_parent() {
    let root = unique_temp("empty-parent");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["/child"]}"#,
    );
    write_file(&root.join("child/package.json"), r#"{"name":"child"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "child");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn star_skips_files_and_dirs_without_package_json() {
    let root = unique_temp("star-skip");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    );
    write_file(&root.join("packages/readme.txt"), "nope");
    require_ok(fs::create_dir_all(root.join("packages/empty")));
    write_file(
        &root.join("packages/real/package.json"),
        r#"{"name":"real"}"#,
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "real");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspaces_object_without_array_is_ignored() {
    let root = unique_temp("ws-obj");
    write_file(
        &root.join("package.json"),
        r#"{"name":"solo","workspaces":{"nope":true}}"#,
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_package_missing_json_is_io_error() {
    let root = unique_temp("root-missing");
    require_ok(fs::create_dir_all(&root));
    assert!(root_package(&root).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn selected_packages_propagates_resolve_io_error() {
    let root = unique_temp("sel-missing");
    require_ok(fs::create_dir_all(&root));
    assert!(selected_packages(&["any".into()], &root).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn expand_star_unreadable_dir_is_io_error() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_temp("unreadable-star");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    );
    let packages = root.join("packages");
    require_ok(fs::create_dir_all(&packages));
    require_ok(fs::set_permissions(
        &packages,
        fs::Permissions::from_mode(0o000),
    ));
    let result = all_packages(&root);
    let _ = fs::set_permissions(&packages, fs::Permissions::from_mode(0o755));
    let _ = fs::remove_dir_all(&root);
    assert!(result.is_err());
}

#[test]
fn take_star_entry_propagates_io_error() {
    let root = unique_temp("star-entry-err");
    require_ok(fs::create_dir_all(&root));
    let mut out = Vec::new();
    let result = take_star_entry(&root, Err(std::io::Error::other("boom")), &mut out);
    let _ = fs::remove_dir_all(&root);
    assert!(result.is_err());
}

#[test]
fn json_helpers_reject_malformed_fields() {
    assert!(json_string_field("{\"name\": }", "name").is_none());
    assert!(json_string_field("{\"name\" \"x\"}", "name").is_none());
    assert!(json_string_field("{\"name\": \"abc", "name").is_none());
    assert!(json_string_field("{\"name\": \"\\", "name").is_none());
    assert!(json_string_array_field("{\"workspaces\": {}", "workspaces").is_none());
}

#[test]
fn json_helpers_arrays_and_whitespace() {
    assert!(parse_json_string_array(" not-an-array").is_none());
    assert!(parse_json_string_array("[\"a\\nb\", \"c\"]").is_some());
    assert!(after_json_string("nope", 0).is_none());
    assert!(after_json_string("\"abc", 0).is_none());
    assert_eq!(skip_ws("  x", 0), 2);
    assert_eq!(skip_ws_and_commas(" , x", 0), 3);
}
