//! End-to-end checks for the `crap-py` binary.

mod common;

use common::{bin, fixture_root, fn_row, has_fn, output_of, parse_json, sample_root};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}

#[test]
fn help_describes_the_tool() {
    let (code, stdout, _) = output_of(bin().arg("--help"));
    assert_eq!(code, 0);
    assert!(stdout.contains("USAGE:"));
    assert!(stdout.contains("--coverage"));
    assert!(stdout.contains("LCOV") || stdout.contains("lcov.info"));
    assert!(stdout.contains("crap-py [OPTIONS]"));
}

#[test]
fn version_prints_crate_name() {
    let (code, stdout, _) = output_of(bin().arg("--version"));
    assert_eq!(code, 0);
    assert!(stdout.contains("crap-py"));
}

#[test]
fn unknown_flag_exits_two() {
    let (code, _, stderr) = output_of(bin().arg("--nope"));
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

#[test]
fn missing_coverage_exits_two() {
    let root = sample_root();
    let (code, _, stderr) =
        output_of(bin().arg("--coverage").arg(root.join("missing.info")).arg("--path").arg(&root));
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

#[test]
fn sample_package_scores_functions() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--summary"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    assert!(stdout.contains("exceed threshold") || stdout.contains("function"));
}

#[test]
fn sample_lcov_joins_with_full_coverage() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    let value = parse_json(&stdout);
    assert_eq!(value["language"], "python");
    for name in ["trivial", "branched"] {
        let row = fn_row(&value, name);
        assert_eq!(row["coverage_percent"], 100.0, "{name}: {row}");
    }
}

#[test]
fn nested_workspace_cover_matches_hits() {
    let root = fixture_root("nested");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--workspace")
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    let value = parse_json(&stdout);
    for name in ["Covered", "PkgFn"] {
        let row = fn_row(&value, name);
        assert_eq!(row["coverage_percent"], 100.0, "{name}: {row}");
    }
}

#[test]
fn basename_util_tie_broken_by_package_name() {
    let root = fixture_root("basename_tie");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--workspace")
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    let value = parse_json(&stdout);
    let hot = fn_row(&value, "Hot");
    let cold = fn_row(&value, "Cold");
    assert_eq!(hot["coverage_percent"], 100.0, "{hot}");
    assert_eq!(cold["coverage_percent"], 0.0, "{cold}");
    assert_eq!(hot["identity"]["crate"], "pkg_a");
    assert_eq!(cold["identity"]["crate"], "pkg_b");
}

fn miss_json(missing: &str) -> (i32, serde_json::Value, String) {
    let root = fixture_root("miss");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json")
            .arg("--missing")
            .arg(missing),
    );
    (code, parse_json(&stdout), stderr)
}

#[test]
fn intentional_miss_is_pessimistic_zero() {
    let (code, value, stderr) = miss_json("pessimistic");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(fn_row(&value, "Covered")["coverage_percent"], 100.0);
    assert_eq!(fn_row(&value, "Missed")["coverage_percent"], 0.0);
}

#[test]
fn intentional_miss_is_optimistic_hundred() {
    let (code, value, stderr) = miss_json("optimistic");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(fn_row(&value, "Covered")["coverage_percent"], 100.0);
    assert_eq!(fn_row(&value, "Missed")["coverage_percent"], 100.0);
}

#[test]
fn intentional_miss_can_skip() {
    let (code, value, stderr) = miss_json("skip");
    assert_eq!(code, 0, "{stderr}");
    assert!(fn_row(&value, "Covered")["coverage_percent"] == 100.0);
    assert!(has_fn(&value, "Covered"));
    assert!(!has_fn(&value, "Missed"));
}

#[test]
fn fail_above_trips_on_high_score() {
    let root = fixture_root("miss");
    let (code, _, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--fail-above")
            .arg("--threshold")
            .arg("0"),
    );
    assert_eq!(code, 1, "{stderr}");
}

#[test]
fn structural_invalid_source_exits_two() {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("crap-py-cli-bad-{n}"));
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"bad\"\n",
    ));
    require_ok(fs::write(root.join("bad.py"), "def f(:\n"));
    require_ok(fs::write(
        root.join("lcov.info"),
        "TN:\nSF:bad.py\nDA:1,1\nend_of_record\n",
    ));
    let (code, _, stderr) =
        output_of(bin().arg("--coverage").arg(root.join("lcov.info")).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stderr}");
    assert!(!stderr.is_empty());
}

#[test]
fn package_flag_selects_one_workspace_member() {
    let root = fixture_root("nested");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("-p")
            .arg("a")
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    let value = parse_json(&stdout);
    assert!(fn_row(&value, "Covered")["coverage_percent"] == 100.0);
    assert!(has_fn(&value, "Covered"));
    assert!(!has_fn(&value, "PkgFn"));
}

#[test]
fn empty_coverage_path_rejected() {
    let root = sample_root();
    let coverage = PathBuf::from("");
    let (code, _, stderr) =
        output_of(bin().arg("--coverage").arg(&coverage).arg("--path").arg(&root));
    assert_eq!(code, 2, "{stderr}");
}
