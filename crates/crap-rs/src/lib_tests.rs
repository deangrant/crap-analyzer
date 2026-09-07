//! Unit tests for [`RustLanguage`] target resolution and collection.

use super::*;
use crap_core::ReportFormat;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn feature_pkg(pairs: &[(&str, &[&str])]) -> Package {
    let mut features = BTreeMap::new();
    for (name, deps) in pairs {
        features.insert(
            (*name).to_owned(),
            deps.iter().map(|dep| (*dep).to_owned()).collect(),
        );
    }
    Package {
        name: "demo".into(),
        root: PathBuf::from("/demo"),
        features,
    }
}

#[test]
fn path_mode_uses_the_request_root() {
    let lang = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection::default(),
    };
    let request = ScanRequest {
        path: PathBuf::from("/proj"),
        coverage: PathBuf::from("lcov.info"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: crap_core::MissingPolicy::Pessimistic,
        format: ReportFormat::Text,
    };
    let targets = lang.resolve_targets(&request);
    assert!(targets.is_ok(), "{targets:?}");
    let targets = targets.unwrap_or_default();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].root, PathBuf::from("/proj"));
    assert!(targets[0].crate_name.is_none());
}

#[test]
fn all_features_uses_package_names() {
    let lang = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection {
            features: Vec::new(),
            all_features: true,
            no_default_features: false,
        },
    };
    let pkg = feature_pkg(&[("std", &[]), ("serde", &[])]);
    assert_eq!(
        lang.enabled_features(Some(&pkg)),
        vec!["serde".to_owned(), "std".to_owned()]
    );
    let cli_only = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection {
            features: vec!["cli".into()],
            all_features: true,
            no_default_features: false,
        },
    };
    assert_eq!(cli_only.enabled_features(None), vec!["cli".to_owned()]);
}

#[test]
fn default_features_close_transitively() {
    let lang = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection::default(),
    };
    let pkg = feature_pkg(&[("default", &["std"]), ("std", &["serde"]), ("serde", &[])]);
    assert_eq!(
        lang.enabled_features(Some(&pkg)),
        vec!["serde".to_owned(), "std".to_owned()]
    );
}

#[path = "tests/feature_collect_tests.rs"]
mod feature_collect;

#[test]
fn path_mode_uses_explicit_features() {
    let lang = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection {
            features: vec!["serde".into()],
            all_features: false,
            no_default_features: false,
        },
    };
    let request = ScanRequest {
        path: PathBuf::from("/proj"),
        coverage: PathBuf::from("lcov.info"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: crap_core::MissingPolicy::Pessimistic,
        format: ReportFormat::Text,
    };
    let targets = lang.resolve_targets(&request).unwrap_or_default();
    assert_eq!(targets[0].enabled_features, vec!["serde".to_owned()]);
}

#[test]
fn collect_targets_propagates_walk_error() {
    let targets = [Target {
        root: PathBuf::from("/no/such/crap-rs-collect-targets"),
        crate_name: None,
        skip: Vec::new(),
        enabled_features: Vec::new(),
    }];
    let mut functions = Vec::new();
    let mut details = Vec::new();
    let result = collect_targets(&targets, Metric::Cyclomatic, &mut functions, &mut details);
    assert!(result.is_err());
}

#[test]
fn parse_failure_includes_path() {
    let detail = parse_failure(Path::new("broken.rs"), &Error::collect("nope"));
    assert!(detail.contains("broken.rs"));
    assert!(detail.contains("nope"));
}

#[test]
fn workspace_root_isolates_members_without_workspace_flag() {
    let lang = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection::default(),
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let request = ScanRequest {
        path: workspace,
        coverage: PathBuf::from("lcov.info"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: crap_core::MissingPolicy::Pessimistic,
        format: ReportFormat::Text,
    };
    let targets = lang.resolve_targets(&request);
    assert!(targets.is_ok(), "{targets:?}");
    let targets = targets.unwrap_or_default();
    let names: Vec<&str> =
        targets.iter().filter_map(|target| target.crate_name.as_deref()).collect();
    assert!(names.len() >= 2 && names.contains(&"crap-core") && names.contains(&"crap-rs"));
}

#[test]
fn member_path_selects_only_that_package() {
    let lang = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection::default(),
    };
    let member = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let request = ScanRequest {
        path: member,
        coverage: PathBuf::from("lcov.info"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: crap_core::MissingPolicy::Pessimistic,
        format: ReportFormat::Text,
    };
    let targets = lang.resolve_targets(&request);
    assert!(targets.is_ok(), "{targets:?}");
    let targets = targets.unwrap_or_default();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].crate_name.as_deref(), Some("crap-rs"));
}

#[test]
fn broken_manifest_is_a_resolve_error() {
    let lang = RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features: FeatureSelection::default(),
    };
    let dir = std::env::temp_dir().join(format!(
        "crap-rs-bad-manifest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let created = std::fs::create_dir_all(&dir);
    assert!(created.is_ok(), "{created:?}");
    let written = std::fs::write(dir.join("Cargo.toml"), "this is not a manifest\n");
    assert!(written.is_ok(), "{written:?}");
    let request = ScanRequest {
        path: dir.clone(),
        coverage: PathBuf::from("lcov.info"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: crap_core::MissingPolicy::Pessimistic,
        format: ReportFormat::Text,
    };
    let targets = lang.resolve_targets(&request);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(targets.is_err(), "{targets:?}");
}

#[test]
fn package_flag_selects_named_members() {
    let lang = RustLanguage {
        workspace: false,
        packages: vec!["crap-rs".into()],
        features: FeatureSelection::default(),
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let request = ScanRequest {
        path: workspace,
        coverage: PathBuf::from("lcov.info"),
        metric: Metric::Cyclomatic,
        threshold: None,
        summary: false,
        fail_above: false,
        missing: crap_core::MissingPolicy::Pessimistic,
        format: ReportFormat::Text,
    };
    let targets = lang.resolve_targets(&request);
    assert!(targets.is_ok(), "{targets:?}");
    let targets = targets.unwrap_or_default();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].crate_name.as_deref(), Some("crap-rs"));
}

#[test]
fn collect_functions_fails_when_any_file_is_unparseable() {
    let dir = std::env::temp_dir().join(format!(
        "crap-rs-collect-mixed-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let created = std::fs::create_dir_all(&dir);
    assert!(created.is_ok(), "{created:?}");
    let ok = std::fs::write(dir.join("ok.rs"), "fn keep() {}\n");
    let bad = std::fs::write(dir.join("broken.rs"), "fn not rust {{{");
    assert!(ok.is_ok() && bad.is_ok(), "{ok:?} {bad:?}");
    let targets = [Target {
        root: dir.clone(),
        crate_name: None,
        skip: Vec::new(),
        enabled_features: Vec::new(),
    }];
    let result = collect_functions(&targets, Metric::Cyclomatic);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "{result:?}");
    let message = format!("{result:?}");
    assert!(message.contains("failed to parse"), "{message}");
    assert!(message.contains("broken.rs"), "{message}");
}
