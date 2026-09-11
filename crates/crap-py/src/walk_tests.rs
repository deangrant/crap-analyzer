use super::{
    Walk, collect_py_file, py_files, take_symlink, take_typed_entry, visit_subdir, walk_entries,
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
    let dir = std::env::temp_dir().join(format!("crap-py-walk-{label}-{nanos}"));
    require_ok(fs::create_dir_all(&dir));
    dir
}

fn write_file(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        require_ok(fs::create_dir_all(parent));
    }
    require_ok(fs::write(path, body));
}

fn touch_py(path: &Path) {
    write_file(path, "def f():\n    pass\n");
}

#[test]
fn collects_py_and_skips_venv_and_pycache() {
    let root = temp_dir("basic");
    touch_py(&root.join("a.py"));
    touch_py(&root.join(".venv/lib/x.py"));
    touch_py(&root.join("venv/lib/x.py"));
    touch_py(&root.join("__pycache__/a.py"));
    touch_py(&root.join("pkg.egg-info/x.py"));
    touch_py(&root.join(".eggs/x.py"));
    touch_py(&root.join("dist/out.py"));
    touch_py(&root.join("build/out.py"));
    touch_py(&root.join(".tox/out.py"));
    touch_py(&root.join("htmlcov/out.py"));
    touch_py(&root.join("coverage/out.py"));
    touch_py(&root.join(".git/x.py"));
    write_file(&root.join("stub.pyi"), "def stub(): ...\n");
    let files = require_ok(py_files(&root, &[]));
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("a.py"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn skips_nested_pyproject_roots() {
    let root = temp_dir("nested");
    touch_py(&root.join("top.py"));
    write_file(
        &root.join("child/pyproject.toml"),
        "[project]\nname = \"child\"\n",
    );
    touch_py(&root.join("child/c.py"));
    let files = require_ok(py_files(&root, &[]));
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("top.py"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn respects_explicit_skip_list() {
    let root = temp_dir("skip");
    touch_py(&root.join("keep.py"));
    let nested = root.join("nested");
    touch_py(&nested.join("gone.py"));
    let files = require_ok(py_files(&root, &[nested]));
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("keep.py"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn collect_py_file_skips_when_canonicalize_fails() {
    let root = temp_dir("canon");
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root: &root,
        root_canon: &root,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    collect_py_file(root.join("x.txt"), &mut walk);
    collect_py_file(root.join("missing.py"), &mut walk);
    assert!(walk.out.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_root_fails_py_files() {
    let root = temp_dir("missing-root");
    let missing = root.join("gone");
    let err = py_files(&missing, &[]);
    let _ = fs::remove_dir_all(&root);
    assert!(matches!(err, Err(crap_core::Error::Io { .. })), "{err:?}");
}

#[test]
fn visit_subdir_skips_missing_outside_and_revisited() {
    let root = temp_dir("subdir");
    touch_py(&root.join("main.py"));
    let outside = temp_dir("outside");
    touch_py(&outside.join("secret.py"));
    let root_canon = require_ok(fs::canonicalize(&root));
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root: &root,
        root_canon: &root_canon,
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
        root_canon: &root,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    let result = take_typed_entry(
        &root.join("x.py"),
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
        root_canon: &root,
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
    fn unreadable_root_fails_py_files() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_dir("unreadable");
        require_ok(fs::set_permissions(
            &root,
            fs::Permissions::from_mode(0o000),
        ));
        let err = py_files(&root, &[]);
        let _ = fs::set_permissions(&root, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&root);
        assert!(matches!(err, Err(crap_core::Error::Io { .. })), "{err:?}");
    }

    #[test]
    fn dangling_symlink_is_ignored() {
        let root = temp_dir("dangling");
        require_ok(symlink(root.join("missing.py"), root.join("link.py")));
        let files = require_ok(py_files(&root, &[]));
        assert!(files.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn directory_symlink_is_followed() {
        let root = temp_dir("dirlink");
        let real = root.join("real");
        touch_py(&real.join("a.py"));
        require_ok(symlink(&real, root.join("link")));
        let files = require_ok(py_files(&root, &[]));
        assert_eq!(files.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn py_file_symlink_is_collected_once() {
        let root = temp_dir("filelink");
        touch_py(&root.join("a.py"));
        require_ok(symlink(root.join("a.py"), root.join("b.py")));
        let files = require_ok(py_files(&root, &[]));
        assert_eq!(files.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn symlink_outside_walk_root_is_skipped() {
        let root = temp_dir("outside-link");
        let outside = temp_dir("secret");
        touch_py(&outside.join("secret.py"));
        require_ok(symlink(&outside, root.join("link")));
        let files = require_ok(py_files(&root, &[]));
        assert!(files.is_empty());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn take_symlink_ignores_non_file_non_dir() {
        let root = temp_dir("sock");
        let mut visited = HashSet::new();
        let mut out = Vec::new();
        let root_canon = require_ok(fs::canonicalize(&root));
        let mut walk = Walk {
            root: &root,
            root_canon: &root_canon,
            nested_skip: &[],
            visited: &mut visited,
            out: &mut out,
        };
        require_ok(take_symlink(&root.join("missing-link"), &mut walk));
        assert!(walk.out.is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
