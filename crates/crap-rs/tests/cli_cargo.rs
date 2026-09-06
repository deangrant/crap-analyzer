//! Cargo metadata override and parse-failure checks for `crap-rs`.

mod common;

use common::{bin, output_of, require_ok, sample_root, workspace_root};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
fn parse_warning_exits_two_when_any_file_fails() {
    let root = std::env::temp_dir().join(format!(
        "crap-rs-mixed-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(root.join("ok.rs"), "fn keep() {}\n"));
    require_ok(fs::write(root.join("broken.rs"), "fn not rust {{{"));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, "TN:\nSF:ok.rs\nDA:1,1\nend_of_record\n"));
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("failed to parse"), "{stderr}");
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
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(root.join("broken.rs"), "fn not rust {{{"));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(
        &lcov,
        "TN:\nSF:broken.rs\nDA:1,0\nend_of_record\n",
    ));
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("failed to parse") || stderr.contains("skipping"));
}
