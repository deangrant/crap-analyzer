//! End-to-end checks for the `crap-rs` binary.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_crap-rs"))
}

fn sample_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn output_of(cmd: &mut Command) -> (i32, String, String) {
    let spawned = cmd.output();
    assert!(spawned.is_ok(), "{spawned:?}");
    let Ok(out) = spawned else {
        return (2, String::new(), String::new());
    };
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

#[test]
fn help_describes_the_tool() {
    let (code, stdout, _) = output_of(bin().arg("--help"));
    assert_eq!(code, 0);
    assert!(stdout.contains("USAGE:"));
    assert!(stdout.contains("--lcov"));
    assert!(stdout.contains("not a quality score"));
    assert!(stdout.contains("crap-rs [OPTIONS]"));
    assert!(stdout.contains("README.md"));
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
            .arg("--lcov")
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
    let created = fs::create_dir_all(&root);
    assert!(created.is_ok(), "{created:?}");
    let written = fs::write(root.join("lib.rs"), "fn f() {}\n");
    assert!(written.is_ok(), "{written:?}");
    let lcov = root.join("lcov.info");
    let lcov_written = fs::write(&lcov, "");
    assert!(lcov_written.is_ok(), "{lcov_written:?}");
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
fn missing_required_flag_exits_two() {
    let (code, _, stderr) = output_of(&mut bin());
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

#[test]
fn missing_cargo_override_falls_back() {
    let (code, stdout, stderr) = output_of(
        bin()
            .env("CARGO", "/no/such/crap-rs-metadata")
            .arg("--lcov")
            .arg(sample_root().join("lcov.info"))
            .arg("--path")
            .arg(workspace_root())
            .arg("-p")
            .arg("crap-rs")
            .arg("--summary"),
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
}

#[test]
fn failing_cargo_override_exits_two() {
    let (code, stderr) = metadata_with_fake_cargo("exit 1");
    assert_eq!(code, 2);
    assert!(stderr.contains("cargo metadata"));
}

fn fake_cargo(dir: &std::path::Path, script: &str) -> PathBuf {
    let path = dir.join("fake-cargo");
    let written = fs::write(&path, format!("#!/bin/sh\n{script}\n"));
    assert!(written.is_ok(), "{written:?}");
    let mode = Command::new("chmod").arg("+x").arg(&path).status();
    assert!(mode.is_ok(), "{mode:?}");
    path
}

fn metadata_with_fake_cargo(script: &str) -> (i32, String) {
    let root = std::env::temp_dir().join(format!(
        "crap-rs-fake-cargo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let created = fs::create_dir_all(&root);
    assert!(created.is_ok(), "{created:?}");
    let cargo = fake_cargo(&root, script);
    let (code, _, stderr) = output_of(
        bin()
            .env("CARGO", &cargo)
            .arg("--lcov")
            .arg(sample_root().join("lcov.info"))
            .arg("-p")
            .arg("crap-rs"),
    );
    let _ = fs::remove_dir_all(&root);
    (code, stderr)
}

#[test]
fn cargo_metadata_non_utf8_exits_two() {
    let (code, stderr) = metadata_with_fake_cargo("printf '\\xff'; exit 0");
    assert_eq!(code, 2);
    assert!(stderr.contains("cargo metadata"));
}

#[test]
fn cargo_metadata_invalid_json_exits_two() {
    let (code, stderr) = metadata_with_fake_cargo("printf 'not-json'; exit 0");
    assert_eq!(code, 2);
    assert!(stderr.contains("cargo metadata"));
}

#[test]
fn parse_warning_exits_zero_when_other_files_succeed() {
    let root = std::env::temp_dir().join(format!(
        "crap-rs-mixed-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let created = fs::create_dir_all(&root);
    assert!(created.is_ok(), "{created:?}");
    let ok = fs::write(root.join("ok.rs"), "fn keep() {}\n");
    assert!(ok.is_ok(), "{ok:?}");
    let broken = fs::write(root.join("broken.rs"), "fn not rust {{{");
    assert!(broken.is_ok(), "{broken:?}");
    let lcov = root.join("lcov.info");
    let lcov_written = fs::write(&lcov, "TN:\nSF:ok.rs\nDA:1,1\nend_of_record\n");
    assert!(lcov_written.is_ok(), "{lcov_written:?}");
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("skipping"));
}

#[test]
fn total_parse_failure_exits_two() {
    let root = std::env::temp_dir().join(format!(
        "crap-rs-unparseable-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let created = fs::create_dir_all(&root);
    assert!(created.is_ok(), "{created:?}");
    let written = fs::write(root.join("broken.rs"), "fn not rust {{{");
    assert!(written.is_ok(), "{written:?}");
    let lcov = root.join("lcov.info");
    let lcov_written = fs::write(&lcov, "TN:\nSF:broken.rs\nDA:1,0\nend_of_record\n");
    assert!(lcov_written.is_ok(), "{lcov_written:?}");
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("failed to parse") || stderr.contains("skipping"));
}
