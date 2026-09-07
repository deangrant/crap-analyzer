use super::*;
use crap_core::{Language, Metric, MissingPolicy, ReportFormat, ScanRequest, Target};
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
    std::env::temp_dir().join(format!("crap-ts-lib-{label}-{n}"))
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
fn resolve_single_package_root() {
    let root = unique_temp("resolve");
    write_file(&root.join("package.json"), r#"{"name":"sample"}"#);
    write_file(
        &root.join("src/main.ts"),
        "export function ok() { return 1; }\n",
    );
    let lang = TsLanguage {
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
fn path_target_without_package_json() {
    let root = unique_temp("bare");
    write_file(
        &root.join("main.ts"),
        "export function bare() { return 1; }\n",
    );
    let lang = TsLanguage {
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
        &root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    );
    write_file(&root.join("packages/a/package.json"), r#"{"name":"a"}"#);
    write_file(
        &root.join("packages/a/src/a.ts"),
        "export function a() { return 1; }\n",
    );
    write_file(&root.join("packages/b/package.json"), r#"{"name":"b"}"#);
    write_file(
        &root.join("packages/b/src/b.ts"),
        "export function b() { return 1; }\n",
    );
    let lang = TsLanguage {
        workspace: true,
        packages: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(root.clone())));
    assert_eq!(targets.len(), 2);
    let selected = TsLanguage {
        workspace: false,
        packages: vec!["b".into()],
    };
    let targets = require_ok(selected.resolve_targets(&request(root.clone())));
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].crate_name.as_deref(), Some("b"));
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn collect_fails_when_any_file_unreadable() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_temp("bad");
    write_file(&root.join("package.json"), r#"{"name":"bad"}"#);
    write_file(
        &root.join("src/ok.ts"),
        "export function ok() { return 1; }\n",
    );
    let bad = root.join("src/bad.ts");
    write_file(&bad, "export function bad() { return 0; }\n");
    require_ok(fs::set_permissions(&bad, fs::Permissions::from_mode(0o000)));
    let lang = TsLanguage {
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
    write_file(&root.join("package.json"), r#"{"name":"ok"}"#);
    write_file(&root.join("ok.ts"), "export function ok() { return 1; }\n");
    let ok = Target {
        root: root.clone(),
        crate_name: Some("ok".into()),
        skip: Vec::new(),
        enabled_features: Vec::new(),
    };
    let bad = Target {
        root: root.join("missing-dir"),
        crate_name: Some("bad".into()),
        skip: Vec::new(),
        enabled_features: Vec::new(),
    };
    // First target succeeds; second target's walk fails and must propagate.
    let result = collect_functions(&[ok, bad], Metric::Cyclomatic);
    let _ = fs::remove_dir_all(&root);
    assert!(result.is_err());
}
