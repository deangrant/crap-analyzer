//! End-to-end checks for the `cargo-crap` binary.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cargo-crap"))
}

fn sample_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample")
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
    assert!(stdout.contains("CRAP(m)"));
    assert!(stdout.contains("not a quality score"));
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
fn missing_lcov_exits_two() {
    let (code, _, stderr) = output_of(bin().arg("--lcov").arg("/no/such/lcov.info"));
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
fn cargo_subcommand_argv_is_accepted() {
    let (code, stdout, _) = output_of(bin().args(["crap", "--help"]));
    assert_eq!(code, 0);
    assert!(stdout.contains("USAGE:"));
}

#[test]
fn total_parse_failure_exits_two() {
    let root = std::env::temp_dir().join(format!(
        "cargo-crap-unparseable-{}-{}",
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
