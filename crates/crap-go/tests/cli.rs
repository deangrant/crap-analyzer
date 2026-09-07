//! End-to-end checks for the `crap-go` binary.

mod common;

use common::{bin, fixture_root, fn_row, has_fn, output_of, parse_json, sample_root};
use std::fs;
use std::path::PathBuf;

#[test]
fn help_describes_the_tool() {
    let (code, stdout, _) = output_of(bin().arg("--help"));
    assert_eq!(code, 0);
    assert!(stdout.contains("USAGE:"));
    assert!(stdout.contains("--coverage"));
    assert!(stdout.contains("coverprofile") || stdout.contains("cover.out"));
    assert!(stdout.contains("crap-go [OPTIONS]"));
}

#[test]
fn version_prints_crate_name() {
    let (code, stdout, _) = output_of(bin().arg("--version"));
    assert_eq!(code, 0);
    assert!(stdout.contains("crap-go"));
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
        output_of(bin().arg("--coverage").arg(root.join("missing.out")).arg("--path").arg(&root));
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

#[test]
fn sample_module_scores_functions() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("cover.out"))
            .arg("--path")
            .arg(&root)
            .arg("--summary"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    assert!(stdout.contains("exceed threshold") || stdout.contains("function"));
}

#[test]
fn import_path_coverprofile_joins_with_non_zero_coverage() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("cover.out"))
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    let value = parse_json(&stdout);
    for name in ["trivial", "branched"] {
        let row = fn_row(&value, name);
        assert_eq!(row["coverage_percent"], 100.0, "{name}: {row}");
    }
}

#[test]
fn nested_module_import_path_cover_matches_hits() {
    let root = fixture_root("nested");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("cover.out"))
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
fn basename_util_tie_broken_by_import_path() {
    let root = fixture_root("basename_tie");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("cover.out"))
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
    assert_eq!(hot["identity"]["crate"], "example.com/tie/pkg_a");
    assert_eq!(cold["identity"]["crate"], "example.com/tie/pkg_b");
}

fn miss_json(missing: &str) -> (i32, serde_json::Value, String) {
    let root = fixture_root("miss");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("cover.out"))
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
fn intentional_miss_is_skipped() {
    let (code, value, stderr) = miss_json("skip");
    assert_eq!(code, 0, "{stderr}");
    assert!(has_fn(&value, "Covered"));
    assert!(!has_fn(&value, "Missed"));
}

#[test]
fn json_without_fail_above_reports_passed_false() {
    let root = fixture_root("nested");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("cover.out"))
            .arg("--path")
            .arg(&root)
            .arg("--workspace")
            .arg("--format")
            .arg("json")
            .arg("--threshold")
            .arg("0.5"),
    );
    assert_eq!(code, 0, "stderr={stderr}\nstdout={stdout}");
    let value = parse_json(&stdout);
    assert_eq!(value["result"]["passed"], false);
    assert_eq!(value["result"]["gate_failed"], false);
    let covered = fn_row(&value, "Covered");
    assert_eq!(covered["exceeds"], true);
}

#[test]
fn sample_with_threshold_and_package() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("cover.out"))
            .arg("--path")
            .arg(&root)
            .arg("--threshold")
            .arg("strict")
            .arg("-p")
            .arg("example.com/sample"),
    );
    assert!(code == 0 || code == 1, "stderr={stderr}\nstdout={stdout}");
    assert!(!stdout.is_empty() || !stderr.is_empty());
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "crap-go-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ))
}

#[cfg(unix)]
#[test]
fn one_unreadable_file_among_many_exits_two() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_root("unreadable");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("go.mod"),
        "module example.com/tmp\n\ngo 1.22\n",
    ));
    require_ok(fs::write(
        root.join("ok.go"),
        "package main\nfunc Ok() {}\n",
    ));
    let bad = root.join("bad.go");
    require_ok(fs::write(&bad, "package main\nfunc Bad() {}\n"));
    require_ok(fs::set_permissions(&bad, fs::Permissions::from_mode(0o000)));
    let cover = root.join("cover.out");
    require_ok(fs::write(
        &cover,
        "mode: set\nexample.com/tmp/ok.go:2.11,2.13 1 1\n",
    ));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(&cover)
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json"),
    );
    let _ = fs::set_permissions(&bad, fs::Permissions::from_mode(0o644));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stdout}{stderr}");
    assert!(stderr.contains("failed to parse"), "{stderr}");
    assert!(!stdout.contains("\"schema_version\""), "{stdout}");
}

#[test]
fn structurally_invalid_source_exits_two() {
    let root = temp_root("struct-bad");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("go.mod"),
        "module example.com/tmp\n\ngo 1.22\n",
    ));
    require_ok(fs::write(
        root.join("ok.go"),
        "package main\nfunc Ok() {}\n",
    ));
    require_ok(fs::write(
        root.join("bad.go"),
        "package main\nfunc Bad() {\n",
    ));
    let cover = root.join("cover.out");
    require_ok(fs::write(
        &cover,
        "mode: set\nexample.com/tmp/ok.go:2.11,2.13 1 1\n",
    ));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(&cover)
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stdout}{stderr}");
    assert!(stderr.contains("failed to parse"), "{stderr}");
    assert!(stderr.contains("unbalanced braces"), "{stderr}");
    assert!(!stdout.contains("\"schema_version\""), "{stdout}");
}

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}
