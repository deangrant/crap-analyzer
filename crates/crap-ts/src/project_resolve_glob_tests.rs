//! JSONC, pnpm, and I/O resolve tests.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
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
fn workspace_multi_segment_star() {
    let root = unique_temp("multistar");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*/libs/*"]}"#,
    );
    write_file(
        &root.join("packages/app/libs/core/package.json"),
        r#"{"name":"core"}"#,
    );
    write_file(&root.join("packages/app/package.json"), r#"{"name":"app"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "core");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_negation_excludes_package() {
    let root = unique_temp("negate");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*","!packages/skip"]}"#,
    );
    write_file(
        &root.join("packages/keep/package.json"),
        r#"{"name":"keep"}"#,
    );
    write_file(
        &root.join("packages/skip/package.json"),
        r#"{"name":"skip"}"#,
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "keep");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn overlapping_globs_dedupe() {
    let root = unique_temp("dedupe");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*","packages/a"]}"#,
    );
    write_file(&root.join("packages/a/package.json"), r#"{"name":"a"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "a");
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
fn workspaces_object_without_packages_is_single() {
    let root = unique_temp("ws-obj");
    write_file(
        &root.join("package.json"),
        r#"{"name":"solo","workspaces":{"nope":true}}"#,
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "solo");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pnpm_workspace_yaml_alone() {
    let root = unique_temp("pnpm-alone");
    write_file(&root.join("package.json"), r#"{"name":"root"}"#);
    write_file(
        &root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    );
    write_file(&root.join("packages/a/package.json"), r#"{"name":"a"}"#);
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "a");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn package_json_workspaces_win_over_pnpm_yaml() {
    let root = unique_temp("pnpm-lose");
    write_file(
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["from-npm/*"]}"#,
    );
    write_file(
        &root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'from-pnpm/*'\n",
    );
    write_file(&root.join("from-npm/a/package.json"), r#"{"name":"npm-a"}"#);
    write_file(
        &root.join("from-pnpm/b/package.json"),
        r#"{"name":"pnpm-b"}"#,
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "npm-a");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn invalid_pnpm_workspace_yaml_is_resolve_error() {
    let root = unique_temp("pnpm-bad");
    write_file(&root.join("package.json"), r#"{"name":"root"}"#);
    write_file(&root.join("pnpm-workspace.yaml"), "catalog:\n  foo: 1\n");
    assert!(all_packages(&root).is_err());
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
