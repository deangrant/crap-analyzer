//! End-to-end checks for the `crap-go` binary.

mod common;

use common::{bin, output_of, sample_root};

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
    assert!(
        stdout.contains("\"coverage_percent\": 100.0"),
        "expected joined 100% coverage for covered functions; got:\n{stdout}"
    );
    assert!(stdout.contains("branched") || stdout.contains("trivial"));
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
