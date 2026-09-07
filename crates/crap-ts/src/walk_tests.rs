use super::{
    Walk, collect_ts_file, take_symlink, take_typed_entry, ts_files, visit_subdir, walk_entries,
};
use std::collections::HashSet;
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
    let dir = std::env::temp_dir().join(format!("crap-ts-walk-{label}-{nanos}"));
    require_ok(fs::create_dir_all(&dir));
    dir
}

fn write_file(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, body));
}

fn touch_ts(path: &Path) {
    write_file(path, "export function f() {}\n");
}

#[test]
fn finds_ts_and_tsx_skips_dts_and_node_modules() {
    let root = temp_dir("basic");
    touch_ts(&root.join("src/a.ts"));
    touch_ts(&root.join("src/b.tsx"));
    require_ok(fs::write(
        root.join("src/types.d.ts"),
        "export type T = number;",
    ));
    touch_ts(&root.join("node_modules/x/x.ts"));
    touch_ts(&root.join("dist/out.ts"));
    touch_ts(&root.join("build/out.ts"));
    touch_ts(&root.join("coverage/out.ts"));
    touch_ts(&root.join(".git/x.ts"));
    let files = require_ok(ts_files(&root, &[]));
    let names: Vec<_> =
        files.iter().filter_map(|p| p.file_name().and_then(|n| n.to_str())).collect();
    assert!(names.contains(&"a.ts"));
    assert!(names.contains(&"b.tsx"));
    assert!(!names.contains(&"types.d.ts"));
    assert!(!names.contains(&"x.ts"));
    assert!(!names.contains(&"out.ts"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn skips_nested_package_roots() {
    let root = temp_dir("nested-skip");
    touch_ts(&root.join("src/root.ts"));
    let nested = root.join("packages/child");
    write_file(&nested.join("package.json"), r#"{"name":"child"}"#);
    touch_ts(&nested.join("src/child.ts"));
    let files = require_ok(ts_files(&root, &[nested]));
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("root.ts"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nested_package_json_without_skip_list_is_skipped() {
    let root = temp_dir("auto-nested");
    touch_ts(&root.join("root.ts"));
    let nested = root.join("nested");
    write_file(&nested.join("package.json"), r#"{"name":"nested"}"#);
    touch_ts(&nested.join("child.ts"));
    let files = require_ok(ts_files(&root, &[]));
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("root.ts"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn collect_ts_file_skips_non_ts_and_keeps_missing() {
    let root = temp_dir("collect");
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root: &root,
        root_canon: None,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    collect_ts_file(root.join("x.js"), &mut walk);
    collect_ts_file(root.join("missing.ts"), &mut walk);
    assert_eq!(walk.out.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn visit_subdir_skips_missing_outside_and_revisited() {
    let root = temp_dir("subdir");
    touch_ts(&root.join("main.ts"));
    let outside = temp_dir("outside");
    touch_ts(&outside.join("secret.ts"));
    let root_canon = require_ok(fs::canonicalize(&root));
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root: &root,
        root_canon: Some(root_canon.as_path()),
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    require_ok(visit_subdir(&root.join("gone"), &mut walk));
    require_ok(visit_subdir(&outside, &mut walk));
    require_ok(visit_subdir(&root, &mut walk));
    let before = walk.out.len();
    require_ok(visit_subdir(&root, &mut walk));
    assert_eq!(walk.out.len(), before);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn take_typed_entry_propagates_file_type_error() {
    let root = temp_dir("ftype");
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root: &root,
        root_canon: None,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    let result = take_typed_entry(
        &root.join("x.ts"),
        Err(std::io::Error::other("boom")),
        &mut walk,
    );
    let _ = fs::remove_dir_all(&root);
    assert!(result.is_err());
}

#[test]
fn walk_entries_propagates_read_dir_error() {
    let root = temp_dir("entries");
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root: &root,
        root_canon: None,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    let result = walk_entries(
        std::iter::once(Err(std::io::Error::other("boom"))),
        &root,
        &mut walk,
    );
    let _ = fs::remove_dir_all(&root);
    assert!(result.is_err());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn dangling_symlink_is_ignored() {
        let root = temp_dir("dangle");
        touch_ts(&root.join("real.ts"));
        require_ok(symlink(root.join("gone.ts"), root.join("alias.ts")));
        let files = require_ok(ts_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert!(files.iter().any(|p| p.ends_with("real.ts")));
        assert!(!files.iter().any(|p| p.ends_with("alias.ts")));
    }

    #[test]
    fn ts_file_symlink_is_collected_once() {
        let root = temp_dir("linkfile");
        touch_ts(&root.join("real.ts"));
        require_ok(symlink(root.join("real.ts"), root.join("alias.ts")));
        let files = require_ok(ts_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn directory_symlink_is_followed() {
        let root = temp_dir("linkdir");
        touch_ts(&root.join("main.ts"));
        let pkg = root.join("pkg");
        touch_ts(&pkg.join("lib.ts"));
        require_ok(symlink(&pkg, root.join("alias")));
        let files = require_ok(ts_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert!(files.len() >= 2);
    }

    #[test]
    fn symlink_outside_walk_root_is_skipped() {
        let root = temp_dir("outlink");
        let outside = temp_dir("secret");
        touch_ts(&outside.join("secret.ts"));
        touch_ts(&root.join("main.ts"));
        require_ok(symlink(outside.join("secret.ts"), root.join("alias.ts")));
        let files = require_ok(ts_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        assert!(files.iter().any(|p| p.ends_with("main.ts")));
        assert!(!files.iter().any(|p| p.ends_with("alias.ts")));
    }

    #[test]
    fn symlink_to_socket_is_ignored() {
        let root = temp_dir("sock");
        touch_ts(&root.join("main.ts"));
        let sock_path = root.join("sock");
        let listener = std::os::unix::net::UnixListener::bind(&sock_path);
        assert!(listener.is_ok(), "{listener:?}");
        require_ok(symlink(&sock_path, root.join("alias.ts")));
        let mut visited = HashSet::new();
        let mut out = Vec::new();
        let mut walk = Walk {
            root: &root,
            root_canon: None,
            nested_skip: &[],
            visited: &mut visited,
            out: &mut out,
        };
        require_ok(take_symlink(&root.join("alias.ts"), &mut walk));
        drop(listener);
        let _ = fs::remove_dir_all(&root);
        assert!(walk.out.is_empty());
    }
}
