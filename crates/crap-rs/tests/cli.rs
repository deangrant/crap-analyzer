//! End-to-end checks for the `crap-rs` binary.

mod common;

use common::{bin, output_of, require_ok, sample_root, workspace_root};
use std::fs;

fn assert_contains(haystack: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            haystack.contains(needle),
            "missing {needle:?} in {haystack}"
        );
    }
}

#[test]
fn help_describes_the_tool() {
    let (code, stdout, _) = output_of(bin().arg("--help"));
    assert_eq!(code, 0);
    assert_contains(
        &stdout,
        &[
            "USAGE:",
            "--coverage",
            "--lcov",
            "not a quality score",
            "crap-rs [OPTIONS]",
            "README.md",
        ],
    );
}

#[test]
fn fail_above_exits_one_when_over_threshold() {
    let root = sample_root();
    let (code, stdout, _) = output_of(
        bin()
            .arg("--lcov")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--fail-above")
            .arg("--threshold")
            .arg("30"),
    );
    assert_eq!(code, 1, "{stdout}");
    assert!(stdout.contains("FAIL"));
    assert!(stdout.contains("crappy"));
}

#[test]
fn without_fail_above_exits_zero() {
    let root = sample_root();
    let (code, _, _) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--threshold")
            .arg("30"),
    );
    assert_eq!(code, 0);
}

#[test]
fn summary_has_no_table_header() {
    let root = sample_root();
    let (code, stdout, _) = output_of(
        bin()
            .arg("--lcov")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--summary"),
    );
    assert_eq!(code, 0);
    assert!(!stdout.contains("FUNCTION"));
    assert!(stdout.contains("exceed threshold"));
}

#[test]
fn empty_lcov_exits_two() {
    let root = std::env::temp_dir().join(format!(
        "crap-rs-empty-lcov-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(root.join("lib.rs"), "fn f() {}\n"));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, ""));
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("DA") || stderr.contains("line-hit"));
}

#[test]
fn missing_lcov_exits_two() {
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg("/no/such/lcov.info"));
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

#[test]
fn missing_default_coverage_exits_two() {
    let (code, _, stderr) = output_of(
        bin()
            .arg("--path")
            .arg("/no/such/crap-rs-path")
            .arg("--coverage")
            .arg("/no/such/lcov.info"),
    );
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

#[test]
fn unknown_package_exits_two() {
    let root = sample_root();
    let (code, _, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(root.join("lcov.info"))
            .arg("-p")
            .arg("definitely-not-a-package"),
    );
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown package"));
}

#[test]
fn fixture_json_locks_sample_scores() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json")
            .arg("--threshold")
            .arg("30"),
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    let parsed = serde_json::from_str::<serde_json::Value>(&stdout);
    assert!(parsed.is_ok(), "{parsed:?}");
    let value = parsed.unwrap_or_default();
    assert_fixture_gate(&value);
    assert_covered_low(&value, "trivial", 1, 1.0);
    assert_covered_low(&value, "moderate", 3, 3.0);
    assert_covered_low(&value, "question", 2, 2.0);
    assert_crappy_high(&value);
}

#[test]
fn fixture_json_fail_above_sets_gate_failed() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json")
            .arg("--threshold")
            .arg("30")
            .arg("--fail-above"),
    );
    assert_eq!(code, 1, "{stdout}{stderr}");
    let parsed = serde_json::from_str::<serde_json::Value>(&stdout);
    assert!(parsed.is_ok(), "{parsed:?}");
    let value = parsed.unwrap_or_default();
    assert_eq!(value["schema_version"], 3);
    assert_eq!(value["result"]["passed"], false);
    assert_eq!(value["result"]["gate_failed"], true);
    let exceeding = value["result"]["summary"]["exceeding"].as_u64().unwrap_or(0);
    assert!(exceeding >= 1, "{exceeding}");
}

fn assert_fixture_gate(value: &serde_json::Value) {
    // Without --fail-above: passed/exceeds still reflect the threshold;
    // gate_failed stays false and the process exits 0.
    assert_eq!(value["schema_version"], 3);
    assert_eq!(value["result"]["passed"], false);
    assert_eq!(value["result"]["gate_failed"], false);
    let exceeding = value["result"]["summary"]["exceeding"].as_u64().unwrap_or(0);
    assert!(exceeding >= 1, "{exceeding}");
}

fn assert_covered_low(value: &serde_json::Value, name: &str, complexity: i64, crap: f64) {
    let row = fixture_fn(value, name);
    let fields = [
        (row["complexity"].clone(), serde_json::json!(complexity)),
        (row["coverage_percent"].clone(), serde_json::json!(100.0)),
        (row["crap"].clone(), serde_json::json!(crap)),
        (row["risk"].clone(), serde_json::json!("low")),
        (row["exceeds"].clone(), serde_json::json!(false)),
    ];
    for (got, want) in fields {
        assert_eq!(got, want);
    }
}

fn assert_crappy_high(value: &serde_json::Value) {
    let row = fixture_fn(value, "crappy");
    let fields = [
        (row["complexity"].clone(), serde_json::json!(11)),
        (row["exceeds"].clone(), serde_json::json!(true)),
        (row["risk"].clone(), serde_json::json!("high")),
    ];
    for (got, want) in fields {
        assert_eq!(got, want);
    }
    let cov = row["coverage_percent"].as_f64().unwrap_or(0.0);
    let crap = row["crap"].as_f64().unwrap_or(0.0);
    assert!(
        (cov - 3.125).abs() < 0.2 && (crap - 121.0).abs() < 1.0,
        "{cov} {crap}"
    );
}

fn fixture_fn<'a>(value: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    value["result"]["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["identity"]["function"] == name)
        .unwrap_or(&serde_json::Value::Null)
}

#[test]
fn garbage_lcov_exits_two() {
    let root = std::env::temp_dir().join(format!(
        "crap-rs-garbage-lcov-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(root.join("lib.rs"), "fn f() {}\n"));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, "this is not lcov\njust noise\n"));
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("DA") || stderr.contains("line-hit"));
}

#[test]
fn json_format_emits_envelope() {
    let root = sample_root();
    let (code, stdout, _) = output_of(
        bin()
            .arg("--lcov")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code, 0);
    assert_contains(
        &stdout,
        &[
            "\"schema_version\"",
            "\"result\"",
            "\"functions\"",
            "\"risk\"",
        ],
    );
    assert!(!stdout.contains("FUNCTION"));
}

#[test]
fn invalid_format_exits_two() {
    let (code, _, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(sample_root().join("lcov.info"))
            .arg("--format")
            .arg("nope"),
    );
    assert_eq!(code, 2);
    assert!(stderr.contains("format") || stderr.contains("invalid"));
}

#[test]
fn leftover_crap_token_exits_two() {
    let (code, _, stderr) = output_of(bin().args(["crap", "--lcov", "x"]));
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

#[test]
fn version_prints_crate_name() {
    let (code, stdout, _) = output_of(bin().arg("--version"));
    assert_eq!(code, 0);
    assert!(stdout.contains("crap-rs"));
}

#[test]
fn selected_package_exits_zero() {
    let workspace = workspace_root();
    let lcov = sample_root().join("lcov.info");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&workspace)
            .arg("-p")
            .arg("crap-rs")
            .arg("--summary"),
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("exceed threshold"));
}

#[test]
fn workspace_flag_exits_zero() {
    let workspace = workspace_root();
    let lcov = sample_root().join("lcov.info");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&workspace)
            .arg("--workspace")
            .arg("--summary"),
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("exceed threshold"));
}
