use super::GoLanguage;
use crap_core::{Error, Language, Metric, ScanRequest};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// dry-rs:ignore-file. shared test temp helpers; parallel shape is intentional.

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("crap-go-lib-{label}-{nanos}"));
    require_ok(fs::create_dir_all(&dir));
    dir
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, body));
}

fn request(root: &Path) -> ScanRequest {
    ScanRequest {
        path: root.to_path_buf(),
        coverage: root.join("cover.out"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: crap_core::MissingPolicy::Pessimistic,
        format: crap_core::ReportFormat::Text,
    }
}

#[test]
fn module_root_resolves_packages() {
    let root = temp_dir("mod");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("main.go"), "package main\nfunc main() {}\n");
    write(&root.join("pkg/lib.go"), "package pkg\nfunc Hi() {}\n");
    let lang = GoLanguage {
        workspace: false,
        packages: Vec::new(),
        tags: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(&root)));
    assert_eq!(targets.len(), 2);
    let fns = require_ok(lang.collect_functions(&targets, Metric::Cyclomatic));
    assert!(fns.iter().any(|f| f.function.name == "main"));
    assert!(fns.iter().any(|f| f.function.name == "Hi"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn path_without_gomod_has_no_crate_name() {
    let root = temp_dir("plain");
    write(&root.join("x.go"), "package x\nfunc F() {}\n");
    let lang = GoLanguage {
        workspace: false,
        packages: Vec::new(),
        tags: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(&root)));
    assert_eq!(targets.len(), 1);
    assert!(targets[0].crate_name.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn workspace_flag_selects_all_packages() {
    let root = temp_dir("ws");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("main.go"), "package main\nfunc main() {}\n");
    write(&root.join("pkg/lib.go"), "package pkg\nfunc Hi() {}\n");
    let lang = GoLanguage {
        workspace: true,
        packages: Vec::new(),
        tags: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(&root)));
    assert_eq!(targets.len(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn package_flag_selects_one_import_path() {
    let root = temp_dir("pkgflag");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("main.go"), "package main\nfunc main() {}\n");
    write(&root.join("pkg/lib.go"), "package pkg\nfunc Hi() {}\n");
    let lang = GoLanguage {
        workspace: false,
        packages: vec!["example.com/demo/pkg".into()],
        tags: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(&root)));
    assert_eq!(targets.len(), 1);
    assert_eq!(
        targets[0].crate_name.as_deref(),
        Some("example.com/demo/pkg")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_tag_skips_file() {
    let root = temp_dir("tags");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(
        &root.join("main.go"),
        "//go:build fancy\n\npackage main\nfunc main() {}\n",
    );
    write(&root.join("ok.go"), "package main\nfunc Ok() {}\n");
    let lang = GoLanguage {
        workspace: false,
        packages: Vec::new(),
        tags: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(&root)));
    let fns = require_ok(lang.collect_functions(&targets, Metric::Cyclomatic));
    assert!(fns.iter().any(|f| f.function.name == "Ok"));
    assert!(!fns.iter().any(|f| f.function.name == "main"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn structurally_invalid_go_file_fails_collect() {
    let root = temp_dir("struct-bad");
    write(&root.join("go.mod"), "module example.com/demo\n");
    write(&root.join("ok.go"), "package main\nfunc Ok() {}\n");
    write(
        &root.join("bad.go"),
        "package main\nfunc Bad() { s := \"oops }\n",
    );
    let lang = GoLanguage {
        workspace: false,
        packages: Vec::new(),
        tags: Vec::new(),
    };
    let targets = require_ok(lang.resolve_targets(&request(&root)));
    let err = lang.collect_functions(&targets, Metric::Cyclomatic);
    let _ = fs::remove_dir_all(&root);
    assert!(matches!(err, Err(Error::Collect(_))), "{err:?}");
    let message = format!("{err:?}");
    assert!(message.contains("failed to parse"), "{message}");
    assert!(message.contains("bad.go"), "{message}");
    assert!(message.contains("unclosed string"), "{message}");
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn unreadable_go_file_fails_collect() {
        let root = temp_dir("unreadable");
        write(&root.join("go.mod"), "module example.com/demo\n");
        write(&root.join("ok.go"), "package main\nfunc Ok() {}\n");
        let bad = root.join("bad.go");
        write(&bad, "package main\nfunc Bad() {}\n");
        require_ok(fs::set_permissions(&bad, fs::Permissions::from_mode(0o000)));
        let lang = GoLanguage {
            workspace: false,
            packages: Vec::new(),
            tags: Vec::new(),
        };
        let targets = require_ok(lang.resolve_targets(&request(&root)));
        let err = lang.collect_functions(&targets, Metric::Cyclomatic);
        let _ = fs::set_permissions(&bad, fs::Permissions::from_mode(0o644));
        let _ = fs::remove_dir_all(&root);
        assert!(matches!(err, Err(Error::Collect(_))), "{err:?}");
        let message = format!("{err:?}");
        assert!(message.contains("failed to parse"), "{message}");
        assert!(message.contains("bad.go"), "{message}");
    }

    #[test]
    fn unreadable_root_fails_collect_targets() {
        let root = temp_dir("badroot");
        require_ok(fs::set_permissions(
            &root,
            fs::Permissions::from_mode(0o000),
        ));
        let lang = GoLanguage {
            workspace: false,
            packages: Vec::new(),
            tags: Vec::new(),
        };
        let request = ScanRequest {
            path: root.clone(),
            coverage: root.join("cover.out"),
            metric: Metric::Cyclomatic,
            threshold: None,
            summary: false,
            fail_above: false,
            missing: crap_core::MissingPolicy::Pessimistic,
            format: crap_core::ReportFormat::Text,
        };
        let targets = require_ok(lang.resolve_targets(&request));
        let err = lang.collect_functions(&targets, Metric::Cyclomatic);
        let _ = fs::set_permissions(&root, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&root);
        assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
    }
}
