use super::{
    Walk, collect_go_file, go_files, take_symlink, take_typed_entry, visit_subdir, walk_entries,
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
    let dir = std::env::temp_dir().join(format!("crap-go-walk-{label}-{nanos}"));
    require_ok(fs::create_dir_all(&dir));
    dir
}

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, "package p\n"));
}

#[test]
fn collects_go_and_skips_tests_vendor_git_testdata() {
    let root = temp_dir("basic");
    touch(&root.join("main.go"));
    touch(&root.join("main_test.go"));
    touch(&root.join("vendor/x/x.go"));
    touch(&root.join(".git/x.go"));
    touch(&root.join("testdata/x.go"));
    touch(&root.join("pkg/lib.go"));
    let files = require_ok(go_files(&root, &[]));
    let names: Vec<_> = files.iter().filter_map(|p| p.strip_prefix(&root).ok()).collect();
    assert!(names.iter().any(|p| *p == Path::new("main.go")));
    assert!(names.iter().any(|p| *p == Path::new("pkg/lib.go")));
    assert!(!names.iter().any(|p| p.to_string_lossy().contains("_test.go")));
    assert!(!names.iter().any(|p| p.starts_with("vendor")));
    assert!(!names.iter().any(|p| p.starts_with("testdata")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn skips_nested_module_roots() {
    let root = temp_dir("nested");
    touch(&root.join("main.go"));
    let nested = root.join("other");
    touch(&nested.join("lib.go"));
    let files = require_ok(go_files(&root, &[nested]));
    assert_eq!(files.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn auto_skips_nested_go_mod_directories() {
    let root = temp_dir("automod");
    touch(&root.join("main.go"));
    write_mod(&root.join("nested"), "module example.com/nested\n");
    touch(&root.join("nested/lib.go"));
    let files = require_ok(go_files(&root, &[]));
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("main.go"));
    let _ = fs::remove_dir_all(&root);
}

fn write_mod(dir: &Path, body: &str) {
    require_ok(fs::create_dir_all(dir));
    require_ok(fs::write(dir.join("go.mod"), body));
}

#[test]
fn collect_go_file_pushes_when_canonicalize_fails() {
    let root = temp_dir("canon");
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root: &root,
        root_canon: None,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    collect_go_file(root.join("missing.go"), &mut walk);
    assert_eq!(walk.out.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn visit_subdir_skips_missing_outside_and_revisited() {
    let root = temp_dir("subdir");
    touch(&root.join("main.go"));
    let outside = temp_dir("outside");
    touch(&outside.join("secret.go"));
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
        &root.join("x.go"),
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
        touch(&root.join("real.go"));
        require_ok(symlink(root.join("gone.go"), root.join("alias.go")));
        let files = require_ok(go_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert!(files.iter().any(|p| p.ends_with("real.go")));
        assert!(!files.iter().any(|p| p.ends_with("alias.go")));
    }

    #[test]
    fn go_file_symlink_is_collected_once() {
        let root = temp_dir("linkfile");
        touch(&root.join("real.go"));
        require_ok(symlink(root.join("real.go"), root.join("alias.go")));
        let files = require_ok(go_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn directory_symlink_is_followed() {
        let root = temp_dir("linkdir");
        touch(&root.join("main.go"));
        let pkg = root.join("pkg");
        touch(&pkg.join("lib.go"));
        require_ok(symlink(&pkg, root.join("alias")));
        let files = require_ok(go_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert!(files.len() >= 2);
    }

    #[test]
    fn symlink_outside_walk_root_is_skipped() {
        let root = temp_dir("outlink");
        let outside = temp_dir("secret");
        touch(&outside.join("secret.go"));
        touch(&root.join("main.go"));
        require_ok(symlink(outside.join("secret.go"), root.join("alias.go")));
        let files = require_ok(go_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        assert!(files.iter().any(|p| p.ends_with("main.go")));
        assert!(!files.iter().any(|p| p.ends_with("alias.go")));
    }

    #[test]
    fn symlink_to_socket_is_ignored() {
        let root = temp_dir("sock");
        touch(&root.join("main.go"));
        let sock_path = root.join("sock");
        let listener = std::os::unix::net::UnixListener::bind(&sock_path);
        assert!(listener.is_ok(), "{listener:?}");
        require_ok(symlink(&sock_path, root.join("alias.go")));
        let mut visited = HashSet::new();
        let mut out = Vec::new();
        let mut walk = Walk {
            root: &root,
            root_canon: None,
            nested_skip: &[],
            visited: &mut visited,
            out: &mut out,
        };
        require_ok(take_symlink(&root.join("alias.go"), &mut walk));
        drop(listener);
        let _ = fs::remove_dir_all(&root);
        assert!(walk.out.is_empty());
    }
}
