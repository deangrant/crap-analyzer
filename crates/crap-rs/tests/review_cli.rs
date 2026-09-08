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

#[test]
fn mixed_parse_without_fail_above_still_exits_two() {
    let root = temp_root("mixed-no-gate");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(root.join("ok.rs"), "fn keep() {}\n"));
    require_ok(fs::write(root.join("broken.rs"), "fn not rust {{{"));
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, "TN:\nSF:ok.rs\nDA:1,1\nend_of_record\n"));
    let (code, stdout, stderr) = output_of(bin().arg("--lcov").arg(&lcov).arg("--path").arg(&root));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 2, "{stdout}{stderr}");
    assert!(stderr.contains("failed to parse"), "{stderr}");
    assert!(!stdout.contains("FUNCTION"), "{stdout}");
}

#[test]
fn debug_assertions_gated_fn_uses_missing_when_lcov_omits_it() {
    let root = temp_root("cfg-debug");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("lib.rs"),
        concat!(
            "fn keep() {}\n\n",
            "#[cfg(debug_assertions)]\n",
            "fn gated() {\n",
            "    let x = 1;\n",
            "    let _ = x;\n",
            "}\n",
        ),
    ));
    // Coverage as if produced without debug_assertions: only `keep` has DA lines.
    let lcov = root.join("lcov.info");
    require_ok(fs::write(&lcov, "TN:\nSF:lib.rs\nDA:1,1\nend_of_record\n"));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--missing")
            .arg("pessimistic")
            .arg("--format")
            .arg("json"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let parsed = serde_json::from_str::<serde_json::Value>(&stdout);
    assert!(parsed.is_ok(), "{parsed:?}");
    let value = parsed.unwrap_or_default();
    let rows = value["result"]["functions"].as_array().cloned().unwrap_or_default();
    let keep = rows.iter().find(|row| row["identity"]["function"] == "keep");
    assert!(keep.is_some(), "{value}");
    #[cfg(debug_assertions)]
    {
        let gated = rows.iter().find(|row| row["identity"]["function"] == "gated");
        assert!(gated.is_some(), "{value}");
        if let Some(row) = gated {
            assert_eq!(row["coverage_percent"], 0.0);
        }
    }
    #[cfg(not(debug_assertions))]
    {
        assert!(
            rows.iter().all(|row| row["identity"]["function"] != "gated"),
            "{value}"
        );
    }
}

#[test]
fn ambiguous_basename_lcov_is_pessimistic_and_gates() {
    let root = temp_root("ambiguous-lib");
    require_ok(fs::create_dir_all(root.join("src")));
    let src = root.join("src/lib.rs");
    require_ok(fs::write(&src, "fn f() {}\n"));
    // Prepend distinct prefixes so both SF keys end with the walked absolute path.
    let lcov = root.join("lcov.info");
    let body = format!(
        "TN:\nSF:/crate_a{}\nDA:1,1\nend_of_record\nTN:\nSF:/crate_b{}\nDA:1,0\nend_of_record\n",
        src.display(),
        src.display(),
    );
    require_ok(fs::write(&lcov, body));
    let (code, stdout, stderr) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json")
            .arg("--fail-above")
            .arg("--threshold")
            .arg("1"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 1, "{stdout}{stderr}");
    let parsed = serde_json::from_str::<serde_json::Value>(&stdout);
    assert!(parsed.is_ok(), "{parsed:?}");
    let value = parsed.unwrap_or_default();
    let row = value["result"]["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["identity"]["function"] == "f")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    assert_eq!(row["coverage_join"], "ambiguous");
    assert_eq!(row["coverage_percent"], 0.0);
    assert_eq!(value["result"]["gate_failed"], true);
    assert_eq!(value["result"]["summary"]["ambiguous"], 1);
}

#[test]
fn tokio_test_harness_is_omitted_under_fail_above() {
    let root = temp_root("tokio-omit");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("lib.rs"),
        concat!(
            "fn keep() {}\n\n",
            "#[tokio::test]\n",
            "fn harness() {\n",
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
            .arg("5")
            .arg("--format")
            .arg("json"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let value = serde_json::from_str::<serde_json::Value>(&stdout).unwrap_or_default();
    let rows = value["result"]["functions"].as_array().cloned().unwrap_or_default();
    assert!(rows.iter().any(|row| row["identity"]["function"] == "keep"));
    assert!(rows.iter().all(|row| row["identity"]["function"] != "harness"));
}

#[test]
fn feature_mismatch_uses_missing_when_enabled() {
    let root = temp_root("feat-mismatch");
    require_ok(fs::create_dir_all(&root));
    require_ok(fs::write(
        root.join("lib.rs"),
        concat!(
            "fn keep() {}\n\n",
            "#[cfg(feature = \"extra\")]\n",
            "fn gated() {\n",
            "    let x = 1;\n",
            "    let _ = x;\n",
            "}\n",
        ),
    ));
    let lcov = root.join("lcov.info");
    // Coverage as if built without `extra`: only `keep` has DA lines.
    require_ok(fs::write(&lcov, "TN:\nSF:lib.rs\nDA:1,1\nend_of_record\n"));

    let (code_off, stdout_off, stderr_off) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--format")
            .arg("json"),
    );
    assert_eq!(code_off, 0, "{stdout_off}{stderr_off}");
    let off = serde_json::from_str::<serde_json::Value>(&stdout_off).unwrap_or_default();
    let off_rows = off["result"]["functions"].as_array().cloned().unwrap_or_default();
    assert!(off_rows.iter().all(|row| row["identity"]["function"] != "gated"));

    let (code_on, stdout_on, stderr_on) = output_of(
        bin()
            .arg("--lcov")
            .arg(&lcov)
            .arg("--path")
            .arg(&root)
            .arg("--features")
            .arg("extra")
            .arg("--missing")
            .arg("pessimistic")
            .arg("--format")
            .arg("json"),
    );
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code_on, 0, "{stdout_on}{stderr_on}");
    let on = serde_json::from_str::<serde_json::Value>(&stdout_on).unwrap_or_default();
    let on_rows = on["result"]["functions"].as_array().cloned().unwrap_or_default();
    let gated = on_rows.iter().find(|row| row["identity"]["function"] == "gated");
    assert!(gated.is_some(), "{on}");
    if let Some(row) = gated {
        assert_eq!(row["coverage_percent"], 0.0);
        assert_eq!(row["coverage_join"], "missing");
    }
}
