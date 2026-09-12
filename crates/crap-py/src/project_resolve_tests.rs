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
    std::env::temp_dir().join(format!("crap-py-resolve-{label}-{n}"))
}

fn write_file(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, body));
}

#[test]
fn root_package_uses_project_name() {
    let root = unique_temp("name");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"sample\"\n",
    );
    let pkg = require_ok(root_package(&root));
    assert_eq!(pkg.name, "sample");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn poetry_name_fallback() {
    let root = unique_temp("poetry");
    write_file(
        &root.join("pyproject.toml"),
        "[tool.poetry]\nname = \"poet\"\n",
    );
    let pkg = require_ok(root_package(&root));
    assert_eq!(pkg.name, "poet");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn fallback_name_uses_directory() {
    let root = unique_temp("fallback-dir");
    write_file(&root.join("pyproject.toml"), "[project]\n");
    let pkg = require_ok(root_package(&root));
    assert_eq!(
        pkg.name,
        root.file_name().and_then(|n| n.to_str()).unwrap_or("package")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_expands_uv_members() {
    let root = unique_temp("ws");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    write_file(
        &root.join("packages/b/pyproject.toml"),
        "[project]\nname = \"b\"\n",
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 2);
    assert_eq!(packages[0].name, "a");
    assert_eq!(packages[1].name, "b");
    let selected = require_ok(selected_packages(&["b".into()], &root));
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "b");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_without_members_is_single() {
    let root = unique_temp("no-members");
    write_file(&root.join("pyproject.toml"), "[project]\nname = \"solo\"\n");
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "solo");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_discovers_nested_without_uv_members() {
    let root = unique_temp("nested-poetry");
    write_file(&root.join("pyproject.toml"), "[project]\nname = \"root\"\n");
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[tool.poetry]\nname = \"a\"\n",
    );
    write_file(
        &root.join("packages/b/pyproject.toml"),
        "[project]\nname = \"b\"\n",
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 3);
    let names: Vec<_> = packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"root"));
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn duplicate_project_names_are_resolve_error() {
    let root = unique_temp("dup-names");
    write_file(&root.join("pyproject.toml"), "[project]\nname = \"root\"\n");
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"same\"\n",
    );
    write_file(
        &root.join("packages/b/pyproject.toml"),
        "[project]\nname = \"same\"\n",
    );
    let err = all_packages(&root);
    assert!(err.is_err(), "{err:?}");
    let msg = format!("{err:?}");
    assert!(msg.contains("duplicate package name"), "{msg}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_double_star_finds_nested() {
    let root = unique_temp("doublestar");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"packages/**\"]\n",
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    write_file(
        &root.join("packages/group/b/pyproject.toml"),
        "[project]\nname = \"b\"\n",
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_double_star_skips_missing_prefix() {
    let root = unique_temp("doublestar-miss");
    write_file(
        &root.join("pyproject.toml"),
        concat!(
            "[project]\nname = \"root\"\n\n",
            "[tool.uv.workspace]\nmembers = [\"ghost/**\", \"packages/*\"]\n",
        ),
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "a");
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn expand_star_unreadable_dir_is_io_error() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_temp("unreadable-star");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
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
fn workspace_negation_excludes_package() {
    let root = unique_temp("neg");
    write_file(
        &root.join("pyproject.toml"),
        concat!(
            "[project]\nname = \"root\"\n\n",
            "[tool.uv.workspace]\nmembers = [\"packages/*\", \"!packages/skip\"]\n",
        ),
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    write_file(
        &root.join("packages/skip/pyproject.toml"),
        "[project]\nname = \"skip\"\n",
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "a");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_exact_path() {
    let root = unique_temp("exact");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"pkg\"]\n",
    );
    write_file(
        &root.join("pkg/pyproject.toml"),
        "[project]\nname = \"pkg\"\n",
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "pkg");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_patterns_match_none_errors() {
    let root = unique_temp("none");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"ghost/*\"]\n",
    );
    assert!(all_packages(&root).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_package_errors() {
    let root = unique_temp("missing");
    write_file(&root.join("pyproject.toml"), "[project]\nname = \"only\"\n");
    let err = selected_packages(&["nope".into()], &root);
    assert!(err.is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn invalid_toml_errors() {
    let root = unique_temp("bad");
    write_file(&root.join("pyproject.toml"), "[[[not toml");
    assert!(root_package(&root).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_package_missing_is_io_error() {
    let root = unique_temp("no-manifest");
    require_ok(fs::create_dir_all(&root));
    assert!(matches!(
        root_package(&root),
        Err(crap_core::Error::Io { .. })
    ));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nested_package_roots_lists_children() {
    let root = PathBuf::from("/tmp/crap-py-nested-roots");
    let all = [
        Package {
            name: "root".into(),
            root: root.clone(),
        },
        Package {
            name: "child".into(),
            root: root.join("child"),
        },
    ];
    let skip = nested_package_roots(&root, &all);
    assert_eq!(skip, vec![root.join("child")]);
}

#[test]
fn star_skips_dirs_without_pyproject() {
    let root = unique_temp("star-skip");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    require_ok(fs::create_dir_all(root.join("packages/empty")));
    write_file(&root.join("packages/file.txt"), "nope\n");
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doublestar_skips_venv() {
    let root = unique_temp("ds-venv");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"packages/**\"]\n",
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    write_file(
        &root.join("packages/.venv/hidden/pyproject.toml"),
        "[project]\nname = \"hidden\"\n",
    );
    let packages = require_ok(all_packages(&root));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "a");
    let _ = fs::remove_dir_all(&root);
}
