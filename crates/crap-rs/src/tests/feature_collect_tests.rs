//! Collect under `default = ["full"]` / `full = ["nested"]`.

use super::*;

fn rust_lang(features: FeatureSelection) -> RustLanguage {
    RustLanguage {
        workspace: false,
        packages: Vec::new(),
        features,
    }
}

fn collected_names(src: &str, features: &[String]) -> Vec<String> {
    let analyzed =
        complexity::analyze_source_features(Path::new("t.rs"), src, Metric::Cyclomatic, features);
    assert!(analyzed.is_ok(), "{analyzed:?}");
    analyzed.unwrap_or_default().into_iter().map(|item| item.name).collect()
}

#[test]
fn default_full_nested_is_collected_unless_no_default_features() {
    let pkg = feature_pkg(&[
        ("default", &["full"]),
        ("full", &["nested"]),
        ("nested", &[]),
    ]);
    let src = "#[cfg(feature = \"nested\")] fn gated() {} fn keep() {}";
    let enabled = rust_lang(FeatureSelection::default()).enabled_features(Some(&pkg));
    assert!(enabled.iter().any(|name| name == "nested"));
    assert_eq!(collected_names(src, &enabled), ["gated", "keep"]);
    let no_default = FeatureSelection {
        features: Vec::new(),
        all_features: false,
        no_default_features: true,
    };
    let disabled = rust_lang(no_default).enabled_features(Some(&pkg));
    assert!(disabled.is_empty());
    assert_eq!(collected_names(src, &disabled), ["keep"]);
}
