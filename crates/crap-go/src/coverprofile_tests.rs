use super::parse_coverprofile;
use crap_core::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("crap-go-{label}-{nanos}"));
    require_ok(fs::create_dir_all(&dir));
    dir
}

fn write_profile(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("cover.out");
    require_ok(fs::write(&path, body));
    path
}

fn assert_coverage_err(path: &Path) {
    let err = parse_coverprofile(path);
    assert!(matches!(err, Err(Error::Coverage(_))), "{err:?}");
}

#[test]
fn missing_profile_is_io_error() {
    let err = parse_coverprofile(Path::new("/no/such/crap-go.cover"));
    assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
}

#[test]
fn invalid_utf8_in_profile_is_io_error() {
    let dir = temp_dir("utf8");
    let path = dir.join("cover.out");
    require_ok(fs::write(&path, b"mode: set\n\xff\nmain.go:1.1,1.2 1 1\n"));
    let err = parse_coverprofile(&path);
    let _ = fs::remove_dir_all(&dir);
    assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
}

#[test]
fn invalid_utf8_mode_line_is_io_error() {
    let dir = temp_dir("utf8-mode");
    let path = dir.join("cover.out");
    require_ok(fs::write(&path, b"\xff"));
    let err = parse_coverprofile(&path);
    let _ = fs::remove_dir_all(&dir);
    assert!(matches!(err, Err(Error::Io { .. })), "{err:?}");
}

#[test]
fn set_mode_caps_hits_at_one() {
    let dir = temp_dir("set");
    let path = write_profile(
        &dir,
        "mode: set\nmain.go:1.1,3.2 2 5\nmain.go:4.1,4.2 1 0\n",
    );
    let files = require_ok(parse_coverprofile(&path));
    let cov = files.get(Path::new("main.go")).cloned().unwrap_or_default();
    assert_eq!(cov.lines.get(&1), Some(&1));
    assert_eq!(cov.lines.get(&2), Some(&1));
    assert_eq!(cov.lines.get(&3), Some(&1));
    assert_eq!(cov.lines.get(&4), Some(&0));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn count_mode_adds_hits() {
    let dir = temp_dir("count");
    let path = write_profile(&dir, "mode: count\na.go:1.1,2.1 1 2\na.go:2.1,2.2 1 3\n");
    let files = require_ok(parse_coverprofile(&path));
    let cov = files.get(Path::new("a.go")).cloned().unwrap_or_default();
    assert_eq!(cov.lines.get(&1), Some(&2));
    assert_eq!(cov.lines.get(&2), Some(&5));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_profile_is_rejected() {
    let dir = temp_dir("empty");
    let path = write_profile(&dir, "mode: set\n");
    assert_coverage_err(&path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn truly_empty_file_is_rejected() {
    let dir = temp_dir("blank");
    let path = write_profile(&dir, "");
    assert_coverage_err(&path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_mode_line_is_rejected() {
    let dir = temp_dir("nomode");
    let path = write_profile(&dir, "main.go:1.1,1.2 1 1\n");
    assert_coverage_err(&path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_mode_is_rejected() {
    let dir = temp_dir("badmode");
    let path = write_profile(&dir, "mode: weird\nmain.go:1.1,1.2 1 1\n");
    assert_coverage_err(&path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn blank_data_lines_are_skipped() {
    let dir = temp_dir("blanks");
    let path = write_profile(&dir, "mode: set\n\nmain.go:1.1,1.2 1 1\n\n");
    let files = require_ok(parse_coverprofile(&path));
    assert_eq!(
        files.get(Path::new("main.go")).and_then(|c| c.lines.get(&1)),
        Some(&1)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn multi_file_profile() {
    let dir = temp_dir("multi");
    let path = write_profile(
        &dir,
        "mode: atomic\npkg/a.go:1.1,1.2 1 1\npkg/b.go:2.1,2.2 1 0\n",
    );
    let files = require_ok(parse_coverprofile(&path));
    assert_eq!(files.len(), 2);
    assert_eq!(
        files.get(Path::new("pkg/a.go")).and_then(|c| c.lines.get(&1)),
        Some(&1)
    );
    assert_eq!(
        files.get(Path::new("pkg/b.go")).and_then(|c| c.lines.get(&2)),
        Some(&0)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn backslash_paths_are_normalized() {
    let dir = temp_dir("slash");
    let path = write_profile(&dir, "mode: set\npkg\\a.go:1.1,1.2 1 1\n");
    let files = require_ok(parse_coverprofile(&path));
    assert!(files.contains_key(Path::new("pkg/a.go")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn malformed_data_lines_are_rejected() {
    let cases = [
        "mode: set\njust-one-token\n",
        "mode: set\ntwo tokens\n",
        "mode: set\nfile-no-colon 1 1\n",
        "mode: set\nfile.go:nospan 1 1\n",
        "mode: set\nfile.go:1,2.3 1 1\n",
        "mode: set\nfile.go:1.2,3 1 1\n",
        "mode: set\nfile.go:x.1,2.3 1 1\n",
        "mode: set\nfile.go:1.1,y.2 1 1\n",
        "mode: set\nfile.go:1.1,2.2 bad 1\n",
        "mode: set\nfile.go:1.1,2.2 1 bad\n",
        "mode: set\nfile.go:0.1,2.2 1 1\n",
        "mode: set\nfile.go:3.1,2.2 1 1\n",
    ];
    for (i, body) in cases.iter().enumerate() {
        let dir = temp_dir(&format!("bad{i}"));
        let path = write_profile(&dir, body);
        assert_coverage_err(&path);
        let _ = fs::remove_dir_all(&dir);
    }
}
