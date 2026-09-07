use super::{parse_coverprofile, remap_import_paths, remap_import_paths_all};
use crap_core::{Error, FileCoverage};
use std::collections::HashMap;
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
fn shared_line_zero_hit_block_is_pessimistic() {
    let dir = temp_dir("shared");
    let path = write_profile(
        &dir,
        "mode: set\nmain.go:10.1,10.8 1 1\nmain.go:10.10,10.14 1 0\n",
    );
    let files = require_ok(parse_coverprofile(&path));
    let cov = files.get(Path::new("main.go")).cloned().unwrap_or_default();
    assert_eq!(cov.lines.get(&10), Some(&0));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn remap_all_prefers_longest_module_prefix() {
    let mut coverage = HashMap::new();
    coverage.insert(
        PathBuf::from("example.com/demo/nested/x.go"),
        FileCoverage {
            lines: std::iter::once((1, 1)).collect(),
        },
    );
    coverage.insert(
        PathBuf::from("example.com/demo/main.go"),
        FileCoverage {
            lines: std::iter::once((1, 1)).collect(),
        },
    );
    let parent = PathBuf::from("/mod");
    let child = PathBuf::from("/mod/nested");
    let remapped = remap_import_paths_all(
        &coverage,
        &[
            (parent.clone(), "example.com/demo".into()),
            (child.clone(), "example.com/demo/nested".into()),
        ],
    );
    assert!(remapped.contains_key(&child.join("x.go")));
    assert!(remapped.contains_key(&parent.join("main.go")));
    assert!(!remapped.contains_key(Path::new("example.com/demo/nested/x.go")));
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

fn file_hits(line: u32, hits: u64) -> FileCoverage {
    FileCoverage {
        lines: std::iter::once((line, hits)).collect(),
    }
}

#[test]
fn remap_rewrites_import_path_keys() {
    let root = PathBuf::from("/proj");
    let coverage = HashMap::from([(PathBuf::from("example.com/demo/pkg/a.go"), file_hits(1, 1))]);
    let remapped = remap_import_paths(&coverage, &root, "example.com/demo");
    assert_eq!(
        remapped.get(Path::new("/proj/pkg/a.go")).and_then(|c| c.lines.get(&1)),
        Some(&1)
    );
}

#[test]
fn remap_leaves_non_module_keys() {
    let root = PathBuf::from("/proj");
    let coverage = HashMap::from([
        (PathBuf::from("main.go"), file_hits(1, 1)),
        (PathBuf::from("/abs/main.go"), file_hits(2, 3)),
        (PathBuf::from("example.com/other/x.go"), file_hits(4, 1)),
    ]);
    let remapped = remap_import_paths(&coverage, &root, "example.com/demo");
    assert!(remapped.contains_key(Path::new("main.go")));
    assert!(remapped.contains_key(Path::new("/abs/main.go")));
    assert!(remapped.contains_key(Path::new("example.com/other/x.go")));
}

#[test]
fn remap_respects_module_prefix_boundary() {
    let root = PathBuf::from("/proj");
    let coverage = HashMap::from([(PathBuf::from("example.com/foobar/x.go"), file_hits(1, 1))]);
    let remapped = remap_import_paths(&coverage, &root, "example.com/foo");
    assert!(remapped.contains_key(Path::new("example.com/foobar/x.go")));
    assert!(!remapped.contains_key(Path::new("/proj/bar/x.go")));
}

#[test]
fn remap_merges_when_keys_collapse() {
    let root = PathBuf::from("/proj");
    let coverage = HashMap::from([
        (PathBuf::from("example.com/demo/a.go"), file_hits(1, 1)),
        (PathBuf::from("/proj/a.go"), file_hits(2, 2)),
    ]);
    let remapped = remap_import_paths(&coverage, &root, "example.com/demo");
    let cov = remapped.get(Path::new("/proj/a.go")).cloned().unwrap_or_default();
    assert_eq!(cov.lines.get(&1), Some(&1));
    assert_eq!(cov.lines.get(&2), Some(&2));
}
