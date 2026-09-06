//! Review lock-in checks for the `crap-rs` binary.

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

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
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

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "crap-rs-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ))
}

fn workspace_lcov() -> PathBuf {
    let lcov = workspace_root().join("lcov.info");
    if lcov.is_file() {
        lcov
    } else {
        sample_root().join("lcov.info")
    }
}

#[test]
fn broken_manifest_without_package_flag_exits_two() {
    let root = temp_root("bad-manifest");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("Cargo.toml"),
        "this is not a manifest\n",
    ));
    require_ok(fs::write(root.join("lib.rs"), "fn keep() {}\n"));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, "TN:\nSF:lib.rs\nDA:1,1\nend_of_record\n"));
    let (code, stdout, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stdout}{stderr}");
    assert!(
        stderr.contains("cargo metadata") || stderr.contains("manifest"),
        "{stderr}"
    );
    assert!(!stdout.contains("FUNCTION"), "{stdout}");
}

#[test]
fn path_member_without_package_flag_is_only_crap_rs() {
    let member = workspace_root().join("crates/crap-rs");
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(workspace_lcov())
            .arg("--path")
            .arg(&member)
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_json_crates_are(&stdout, "crap-rs");
}

fn assert_json_crates_are(stdout: &str, name: &str) {
    let parsed = serde_json::from_str::<serde_json::Value>(stdout);
    assert!(parsed.is_ok(), "{parsed:?}");
    let value = parsed.unwrap_or_default();
    let rows = value["result"]["functions"].as_array();
    assert!(rows.is_some_and(|rows| !rows.is_empty()), "{value}");
    for row in rows.into_iter().flatten() {
        assert_eq!(row["identity"]["crate"], name, "{row}");
    }
}

#[test]
fn unknown_cfg_does_not_trip_fail_above() {
    let root = temp_root("cfg-weird");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("lib.rs"),
        concat!(
            "fn keep() {}\n\n",
            "#[cfg(weird)]\n",
            "fn gated() {\n",
            "    if true {\n",
            "        if true {\n",
            "            if true {\n",
            "                if true {\n",
            "                    if true {\n",
            "                        if true {}\n",
            "                    }\n",
            "                }\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ),
    ));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, "TN:\nSF:lib.rs\nDA:1,1\nend_of_record\n"));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--fail-above")
            .arg("--threshold")
            .arg("5"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 0, "{stdout}{stderr}");
}

#[test]
fn mixed_parse_with_fail_above_still_exits_two() {
    let root = temp_root("mixed-fail-above");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("ok.rs"),
        concat!(
            "fn keep() {\n",
            "    if true {\n",
            "        if true {\n",
            "            if true {\n",
            "                if true {\n",
            "                    if true {}\n",
            "                }\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ),
    ));
    require_ok(fs::write(root.join("broken.rs"), "fn not rust {{{"));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, "TN:\nSF:ok.rs\nDA:1,0\nend_of_record\n"));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--fail-above")
            .arg("--threshold")
            .arg("5"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stdout}{stderr}");
    assert!(stderr.contains("failed to parse"), "{stderr}");
}
