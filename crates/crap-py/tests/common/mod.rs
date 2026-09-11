//! Shared helpers for `crap-py` integration tests.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

pub fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_crap-py"))
}

pub fn sample_root() -> PathBuf {
    fixture_root("sample")
}

pub fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

pub fn output_of(cmd: &mut Command) -> (i32, String, String) {
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

pub fn parse_json(stdout: &str) -> Value {
    let parsed = serde_json::from_str(stdout);
    assert!(parsed.is_ok(), "{parsed:?}\n{stdout}");
    parsed.unwrap_or(Value::Null)
}

pub fn fn_row<'a>(value: &'a Value, name: &str) -> &'a Value {
    let rows = value["result"]["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["identity"]["function"] == name);
    assert!(rows.is_some(), "missing function {name} in {value}");
    rows.unwrap_or(&Value::Null)
}

pub fn has_fn(value: &Value, name: &str) -> bool {
    value["result"]["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|row| row["identity"]["function"] == name)
}
