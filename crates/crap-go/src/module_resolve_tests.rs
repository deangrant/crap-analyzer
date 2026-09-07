use super::{
    all_packages, enclosing_module, import_path, modules_for_remap, nested_module_roots,
    push_dir_entry, selected_packages, take_package_dir,
};
use crap_core::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("crap-go-mod-{label}-{nanos}"));
    require_ok(fs::create_dir_all(&dir));
    dir
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, body));
}

#[test]
fn discovers_packages_including_nested_module() {
    let root = temp_dir("all");
    write(&root.join("go.mod"), "module example.com/demo\n\ngo 1.22\n");
    write(&root.join("main.go"), "package main\n");
    write(&root.join("pkg/lib.go"), "package pkg\n");
    write(&root.join("nested/go.mod"), "module example.com/other\n");
    write(&root.join("nested/x.go"), "package nested\n");
    write(&root.join("vendor/v/v.go"), "package v\n");
    let packages = require_ok(all_packages(&root));
    let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"example.com/demo"));
    assert!(names.contains(&"example.com/demo/pkg"));
    assert!(names.contains(&"example.com/other"));
    assert!(!names.iter().any(|n| n.contains("vendor")));
    let demo_root = packages
        .iter()
        .find(|p| p.name == "example.com/demo")
        .map_or(root.as_path(), |p| p.root.as_path());
    let nested = nested_module_roots(demo_root, &packages);
    assert!(nested.iter().any(|p| p.ends_with("pkg")));
    assert!(nested.iter().any(|p| p.ends_with("nested")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn selected_packages_by_import_path() {
    let root = temp_dir("sel");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("main.go"), "package main\n");
    write(&root.join("pkg/lib.go"), "package pkg\n");
    let selected = require_ok(selected_packages(&["example.com/demo/pkg".into()], &root));
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "example.com/demo/pkg");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_package_is_an_error() {
    let root = temp_dir("unk");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("main.go"), "package main\n");
    let err = selected_packages(&["example.com/missing".into()], &root);
    assert!(matches!(err, Err(Error::Resolve(_))), "{err:?}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn go_mod_without_module_line_is_an_error() {
    let root = temp_dir("nomod");
    write(&root.join("go.mod"), "go 1.22\nmodule\n");
    let err = all_packages(&root);
    assert!(matches!(err, Err(Error::Resolve(_))), "{err:?}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dir_with_only_tests_is_not_a_package() {
    let root = temp_dir("tests-only");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("x_test.go"), "package x\n");
    let packages = require_ok(all_packages(&root));
    assert!(packages.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn import_path_falls_back_when_not_under_module_root() {
    let name = import_path(Path::new("/other"), Path::new("/mod"), "example.com/demo");
    assert_eq!(name, "example.com/demo");
}

#[test]
fn take_package_dir_propagates_entry_error() {
    let root = temp_dir("walk-err");
    let err = take_package_dir(
        Err(std::io::Error::other("boom")),
        &root,
        &root,
        "example.com/demo",
        &mut Vec::new(),
    );
    let _ = fs::remove_dir_all(&root);
    assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
}

#[test]
fn push_dir_entry_propagates_error() {
    let root = temp_dir("push-err");
    let err = push_dir_entry(Err(std::io::Error::other("boom")), &root, &mut Vec::new());
    let _ = fs::remove_dir_all(&root);
    assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
}

#[test]
fn missing_go_mod_is_io_error() {
    let root = temp_dir("nogo");
    let err = all_packages(&root);
    let _ = fs::remove_dir_all(&root);
    assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
}

#[test]
fn enclosing_module_finds_parent_go_mod() {
    let root = temp_dir("enclose");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("pkg/lib.go"), "package pkg\n");
    let found = require_ok(enclosing_module(&root.join("pkg")));
    assert!(found.is_some());
    if let Some((mod_root, module)) = found {
        assert_eq!(mod_root, root);
        assert_eq!(module, "example.com/demo");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn enclosing_module_returns_none_without_go_mod() {
    let root = temp_dir("noenclose");
    write(&root.join("x.go"), "package x\n");
    let found = require_ok(enclosing_module(&root));
    assert!(found.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn modules_for_remap_falls_back_to_enclosing() {
    let root = temp_dir("remap-encl");
    write(&root.join("go.mod"), "module example.com/demo\n");
    let nested = root.join("subdir");
    require_ok(fs::create_dir(&nested));
    let modules = require_ok(modules_for_remap(&nested));
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].0, root);
    assert_eq!(modules[0].1, "example.com/demo");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn modules_for_remap_empty_without_module() {
    let root = temp_dir("remap-empty");
    let modules = require_ok(modules_for_remap(&root));
    assert!(modules.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn unreadable_subdir_surfaces_io_error() {
        let root = temp_dir("perm");
        write(&root.join("go.mod"), "module example.com/demo\n");
        write(&root.join("main.go"), "package main\n");
        let locked = root.join("locked");
        require_ok(fs::create_dir(&locked));
        require_ok(fs::set_permissions(
            &locked,
            fs::Permissions::from_mode(0o000),
        ));
        let err = all_packages(&root);
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&root);
        assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
    }
}
