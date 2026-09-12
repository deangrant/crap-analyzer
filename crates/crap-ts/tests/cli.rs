//! End-to-end checks for the `crap-ts` binary.

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
    assert!(stdout.contains("LCOV") || stdout.contains("lcov.info"));
    assert!(stdout.contains("crap-ts [OPTIONS]"));
}

#[test]
fn version_prints_crate_name() {
    let (code, stdout, _) = output_of(bin().arg("--version"));
    assert_eq!(code, 0);
    assert!(stdout.contains("crap-ts"));
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
    assert_eq!(value["language"], "typescript");
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
fn intentional_miss_is_skipped() {
    let (code, value, stderr) = miss_json("skip");
    assert_eq!(code, 0, "{stderr}");
    assert!(has_fn(&value, "Covered"));
    assert!(!has_fn(&value, "Missed"));
}

#[test]
fn json_without_fail_above_reports_threshold_not_cleared() {
    let root = fixture_root("nested");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
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
    assert_eq!(value["result"]["threshold_cleared"], false);
    assert_eq!(value["result"]["gate_failed"], false);
}

#[test]
fn sample_with_threshold_and_package() {
    let root = sample_root();
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(root.join("lcov.info"))
            .arg("--path")
            .arg(&root)
            .arg("--threshold")
            .arg("strict")
            .arg("-p")
            .arg("sample"),
    );
    assert!(code == 0 || code == 1, "stderr={stderr}\nstdout={stdout}");
    assert!(!stdout.is_empty() || !stderr.is_empty());
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "crap-ts-{label}-{}-{}",
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
    require_ok(fs::create_dir_all(root.join("src")));
    require_ok(fs::write(root.join("package.json"), r#"{"name":"tmp"}"#));
    require_ok(fs::write(
        root.join("src/ok.ts"),
        "export function Ok() { return 1; }\n",
    ));
    let bad = root.join("src/bad.ts");
    require_ok(fs::write(&bad, "export function Bad() { return 0; }\n"));
    require_ok(fs::set_permissions(&bad, fs::Permissions::from_mode(0o000)));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(
        &lcov,
        "TN:\nSF:src/ok.ts\nDA:1,1\nDA:2,1\nend_of_record\n",
    ));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(&lcov)
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
    require_ok(fs::create_dir_all(root.join("src")));
    require_ok(fs::write(root.join("package.json"), r#"{"name":"tmp"}"#));
    require_ok(fs::write(
        root.join("src/ok.ts"),
        "export function Ok() { return 1; }\n",
    ));
    require_ok(fs::write(
        root.join("src/bad.ts"),
        "export function Bad() {\n",
    ));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(
        &lcov,
        "TN:\nSF:src/ok.ts\nDA:1,1\nDA:2,1\nend_of_record\n",
    ));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(&lcov)
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

#[test]
fn scoped_package_lcov_joins_via_join_key() {
    let root = temp_root("scoped");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/@scope/*"]}"#,
    ));
    let pkg = root.join("packages/@scope/pkg");
    require_ok(fs::create_dir_all(pkg.join("src")));
    require_ok(fs::write(
        pkg.join("package.json"),
        r#"{"name":"@scope/pkg"}"#,
    ));
    require_ok(fs::write(
        pkg.join("src/index.ts"),
        "export function Scoped() { return 1; }\n",
    ));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(
        &lcov,
        "TN:\nSF:packages/@scope/pkg/src/index.ts\nDA:1,1\nDA:2,1\nend_of_record\n",
    ));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--workspace")
            .arg("--format")
            .arg("json"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let value = parse_json(&stdout);
    let row = fn_row(&value, "Scoped");
    assert_eq!(row["coverage_percent"], 100.0, "{row}");
    assert_eq!(row["coverage_join"], "measured");
    assert_eq!(row["identity"]["crate"], "@scope/pkg");
}

#[test]
fn leftover_ambiguous_basename_gates_under_fail_above() {
    let root = temp_root("ambig-tie");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"]}"#,
    ));
    let sources = write_ambig_packages(&root);
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, ambig_lcov_body(&sources)));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--workspace")
            .arg("--format")
            .arg("json")
            .arg("--fail-above")
            .arg("--threshold")
            .arg("1"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_ambig_gate(&parse_json(&stdout));
}

fn write_ambig_packages(root: &std::path::Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for (pkg, body) in [
        ("pkg_a", "export function Hot() { return 1; }\n"),
        ("pkg_b", "export function Cold() { return 0; }\n"),
    ] {
        let dir = root.join("packages").join(pkg).join("src");
        require_ok(fs::create_dir_all(&dir));
        require_ok(fs::write(
            root.join("packages").join(pkg).join("package.json"),
            format!(r#"{{"name":"{pkg}"}}"#),
        ));
        let src = dir.join("index.ts");
        require_ok(fs::write(&src, body));
        sources.push(src);
    }
    sources
}

fn ambig_lcov_body(sources: &[PathBuf]) -> String {
    use std::fmt::Write;
    let mut body = String::new();
    for src in sources {
        let _ = write!(
            body,
            "TN:\nSF:/vendor/a{}\nDA:1,1\nDA:2,1\nend_of_record\n",
            src.display()
        );
        let _ = write!(
            body,
            "TN:\nSF:/vendor/b{}\nDA:1,0\nDA:2,0\nend_of_record\n",
            src.display()
        );
    }
    body
}

fn assert_ambig_gate(value: &serde_json::Value) {
    assert_eq!(value["result"]["gate_failed"], true);
    let ambiguous = value["result"]["summary"]["ambiguous"].as_u64().unwrap_or(0);
    assert!(ambiguous >= 1, "{value}");
    for name in ["Hot", "Cold"] {
        let row = fn_row(value, name);
        assert_eq!(row["coverage_join"], "ambiguous", "{name}: {row}");
        assert_eq!(row["coverage_percent"], 0.0, "{name}: {row}");
    }
}

#[test]
fn nonsense_balanced_source_still_collects() {
    let root = temp_root("nonsense-ok");
    require_ok(fs::create_dir_all(root.join("src")));
    require_ok(fs::write(root.join("package.json"), r#"{"name":"tmp"}"#));
    require_ok(fs::write(
        root.join("src/main.ts"),
        "export function Weird() { notValidTypeScript $$$ { nested } }\n",
    ));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(
        &lcov,
        "TN:\nSF:src/main.ts\nDA:1,1\nend_of_record\n",
    ));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--coverage")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(has_fn(&parse_json(&stdout), "Weird"));
}

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}
