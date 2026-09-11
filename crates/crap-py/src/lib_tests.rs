use super::*;
use crap_core::{Language, Metric, MissingPolicy, ReportFormat, ScanRequest};
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
    std::env::temp_dir().join(format!("crap-py-lib-{label}-{n}"))
}

fn write_file(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, body));
}

fn request(path: PathBuf) -> ScanRequest {
    ScanRequest {
        path,
        coverage: PathBuf::from("lcov.info"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: MissingPolicy::Pessimistic,
        format: ReportFormat::Text,
    }
}

#[test]
fn resolve_single_project_root() {
    let root = unique_temp("resolve");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"sample\"\n",
    );
    write_file(&root.join("main.py"), "def ok():\n    return 1\n");
    let lang = PyLanguage {
        workspace: false,
        packages: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(root.clone())));
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].crate_name.as_deref(), Some("sample"));
    let fns = require_ok(lang.collect_functions(&targets, Metric::Cyclomatic));
    assert!(fns.iter().any(|f| f.function.name == "ok"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn path_target_without_pyproject() {
    let root = unique_temp("bare");
    write_file(&root.join("main.py"), "def bare():\n    return 1\n");
    let lang = PyLanguage {
        workspace: false,
        packages: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(root.clone())));
    assert_eq!(targets.len(), 1);
    assert!(targets[0].crate_name.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_selection_expands_packages() {
    let root = unique_temp("ws");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    write_file(&root.join("packages/a/a.py"), "def a():\n    return 1\n");
    write_file(
        &root.join("packages/b/pyproject.toml"),
        "[project]\nname = \"b\"\n",
    );
    write_file(&root.join("packages/b/b.py"), "def b():\n    return 1\n");
    let lang = PyLanguage {
        workspace: true,
        packages: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(root.clone())));
    assert_eq!(targets.len(), 2);
    let fns = require_ok(lang.collect_functions(&targets, Metric::Cyclomatic));
    assert!(fns.iter().any(|f| f.function.name == "a"));
    assert!(fns.iter().any(|f| f.function.name == "b"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn selected_package_by_name() {
    let root = unique_temp("sel");
    write_file(
        &root.join("pyproject.toml"),
        "[project]\nname = \"root\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
    );
    write_file(
        &root.join("packages/a/pyproject.toml"),
        "[project]\nname = \"a\"\n",
    );
    write_file(&root.join("packages/a/a.py"), "def a():\n    return 1\n");
    write_file(
        &root.join("packages/b/pyproject.toml"),
        "[project]\nname = \"b\"\n",
    );
    write_file(&root.join("packages/b/b.py"), "def b():\n    return 1\n");
    let lang = PyLanguage {
        workspace: false,
        packages: vec!["a".into()],
    };
    let targets = require_ok(lang.resolve_targets(&request(root.clone())));
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].crate_name.as_deref(), Some("a"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn relative_join_key_filters_root_and_falls_back() {
    assert_eq!(
        relative_join_key(Path::new("/workspace"), Path::new("/other/pkg")),
        "other/pkg"
    );
    assert_eq!(
        relative_join_key(Path::new("/workspace"), Path::new("/")),
        "."
    );
}

#[cfg(unix)]
#[test]
fn collect_fails_when_any_file_unreadable() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_temp("unread");
    write_file(&root.join("pyproject.toml"), "[project]\nname = \"bad\"\n");
    write_file(&root.join("ok.py"), "def ok():\n    return 1\n");
    let bad = root.join("bad.py");
    write_file(&bad, "def bad():\n    return 0\n");
    require_ok(fs::set_permissions(&bad, fs::Permissions::from_mode(0o000)));
    let lang = PyLanguage {
        workspace: false,
        packages: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(root.clone())));
    let result = lang.collect_functions(&targets, Metric::Cyclomatic);
    let _ = fs::set_permissions(&bad, fs::Permissions::from_mode(0o644));
    let _ = fs::remove_dir_all(&root);
    assert!(result.is_err());
}

#[test]
fn collect_propagates_walk_errors_across_targets() {
    let root = unique_temp("walk-err");
    write_file(&root.join("pyproject.toml"), "[project]\nname = \"bad\"\n");
    let lang = PyLanguage {
        workspace: false,
        packages: Vec::new(),
    };
    let mut targets = require_ok(lang.resolve_targets(&request(root.clone())));
    targets[0].root = root.join("missing-subdir");
    let err = lang.collect_functions(&targets, Metric::Cyclomatic);
    let _ = fs::remove_dir_all(&root);
    assert!(err.is_err());
}
