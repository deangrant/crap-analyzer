//! Shared helpers for `crap-go` integration tests.

use std::path::PathBuf;
use std::process::Command;

pub fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_crap-go"))
}

pub fn sample_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample")
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
