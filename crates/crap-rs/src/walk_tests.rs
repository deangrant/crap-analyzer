use super::*;

fn assert_skip<'a>(
    cases: impl IntoIterator<Item = (bool, &'a Path, &'a Path, Option<&'a Path>, &'a [PathBuf])>,
) {
    for (want, path, walk, pkg, nested) in cases {
        assert_eq!(
            skip_dir(path, walk, pkg, nested),
            want,
            "{}",
            path.display()
        );
    }
}

#[test]
fn convention_dirs_skip_tests_everywhere() {
    let walk = Path::new("/proj");
    let pkg = Path::new("/proj/crates/foo");
    let empty: &[PathBuf] = &[];
    assert_skip([
        (
            true,
            Path::new("/proj/crates/foo/tests"),
            walk,
            Some(pkg),
            empty,
        ),
        (
            true,
            Path::new("/proj/crates/foo/benches"),
            walk,
            Some(pkg),
            empty,
        ),
        (
            true,
            Path::new("/proj/crates/foo/examples"),
            walk,
            Some(pkg),
            empty,
        ),
        (
            true,
            Path::new("/proj/crates/foo/src/tests"),
            walk,
            Some(pkg),
            empty,
        ),
        (
            true,
            Path::new("/proj/crates/tests"),
            walk,
            Some(Path::new("/proj")),
            empty,
        ),
        (false, pkg, walk, Some(pkg), empty),
    ]);
}

#[test]
fn skip_dir_ignores_target_and_nested_members() {
    let root = Path::new("/proj");
    let nested = [PathBuf::from("/proj/inner")];
    let empty: &[PathBuf] = &[];
    assert_skip([
        (
            true,
            Path::new("/proj/target"),
            root,
            None,
            nested.as_slice(),
        ),
        (
            true,
            Path::new("/proj/inner"),
            root,
            None,
            nested.as_slice(),
        ),
        (false, root, root, None, nested.as_slice()),
        (false, Path::new("/proj/src"), root, None, nested.as_slice()),
        (false, Path::new("/"), root, None, empty),
    ]);
}

fn temp_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "crap-rs-walk-pkg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let created = fs::create_dir_all(&dir);
    assert!(created.is_ok(), "{created:?}");
    dir
}

fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}

#[test]
fn rust_files_skips_src_tests_and_package_tests() {
    let root = temp_root();
    require_ok(fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    ));
    let src_tests = root.join("src/tests");
    require_ok(fs::create_dir_all(&src_tests));
    require_ok(fs::write(src_tests.join("helper.rs"), "fn h() {}\n"));
    require_ok(fs::create_dir_all(root.join("src")));
    require_ok(fs::write(root.join("src/tests.rs"), "fn t() {}\n"));
    require_ok(fs::write(root.join("src/lib.rs"), "fn prod() {}\n"));
    let integ_dir = root.join("tests");
    require_ok(fs::create_dir_all(&integ_dir));
    require_ok(fs::write(integ_dir.join("integration.rs"), "fn i() {}\n"));
    let files = require_ok(rust_files(&root, &[]));
    let _ = fs::remove_dir_all(&root);
    let names = ["helper.rs", "tests.rs", "integration.rs", "lib.rs"];
    let found = names.map(|name| files.iter().any(|path| path.ends_with(name)));
    assert_eq!(found, [false, false, false, true]);
}

#[test]
fn rust_files_skips_star_tests_rs() {
    let root = temp_root();
    require_ok(fs::create_dir_all(root.join("src")));
    require_ok(fs::write(root.join("src/lib.rs"), "fn prod() {}\n"));
    require_ok(fs::write(root.join("src/foo_tests.rs"), "fn helper() {}\n"));
    let files = require_ok(rust_files(&root, &[]));
    let _ = fs::remove_dir_all(&root);
    let found =
        ["lib.rs", "foo_tests.rs"].map(|name| files.iter().any(|path| path.ends_with(name)));
    assert_eq!(found, [true, false]);
}

#[test]
fn collect_rust_file_skips_non_rust_and_keeps_missing() {
    let root = Path::new("/proj");
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root,
        root_canon: None,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    collect_rust_file(PathBuf::from("notes.txt"), &mut walk);
    collect_rust_file(PathBuf::from("/no/such/crap-rs-missing.rs"), &mut walk);
    assert_eq!(out.len(), 1);
    assert!(out[0].ends_with("crap-rs-missing.rs"));
}

#[test]
fn walk_entries_propagates_read_dir_error() {
    let err = std::io::Error::other("boom");
    let root = Path::new("/proj");
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    let mut walk = Walk {
        root,
        root_canon: None,
        nested_skip: &[],
        visited: &mut visited,
        out: &mut out,
    };
    let result = walk_entries(std::iter::once(Err(err)), root, None, &mut walk);
    assert!(result.is_err());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn directory_symlink_cycle_does_not_hang() {
        let root = temp_root();
        require_ok(fs::write(root.join("lib.rs"), "fn f() {}\n"));
        let loop_dir = root.join("loop");
        require_ok(fs::create_dir(&loop_dir));
        require_ok(symlink(&loop_dir, loop_dir.join("back")));
        let files = require_ok(rust_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert!(files.iter().any(|path| path.ends_with("lib.rs")));
    }

    #[test]
    fn dangling_symlink_is_ignored() {
        let root = temp_root();
        require_ok(fs::write(root.join("real.rs"), "fn f() {}\n"));
        require_ok(symlink(root.join("gone.rs"), root.join("alias.rs")));
        let files = require_ok(rust_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        let found =
            ["real.rs", "alias.rs"].map(|name| files.iter().any(|path| path.ends_with(name)));
        assert_eq!(found, [true, false]);
    }

    #[test]
    fn rust_file_symlink_is_collected_once() {
        let root = temp_root();
        require_ok(fs::write(root.join("real.rs"), "fn f() {}\n"));
        require_ok(symlink(root.join("real.rs"), root.join("alias.rs")));
        let files = require_ok(rust_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(files.len(), 1);
        assert!(
            files[0].ends_with("real.rs") || files[0].ends_with("alias.rs"),
            "{files:?}"
        );
    }

    #[test]
    fn symlink_outside_walk_root_is_skipped() {
        let root = temp_root();
        let outside = temp_root();
        require_ok(fs::write(outside.join("secret.rs"), "fn leak() {}\n"));
        require_ok(fs::write(root.join("lib.rs"), "fn f() {}\n"));
        require_ok(symlink(outside.join("secret.rs"), root.join("alias.rs")));
        let files = require_ok(rust_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        assert!(files.iter().any(|path| path.ends_with("lib.rs")));
        assert!(!files.iter().any(|path| path.ends_with("alias.rs")));
        assert!(!files.iter().any(|path| path.ends_with("secret.rs")));
    }

    #[test]
    fn nested_skip_of_root_yields_no_files() {
        let root = temp_root();
        let written = fs::write(root.join("lib.rs"), "fn f() {}\n");
        assert!(written.is_ok(), "{written:?}");
        let files = rust_files(&root, std::slice::from_ref(&root));
        let _ = fs::remove_dir_all(&root);
        assert!(files.is_ok(), "{files:?}");
        assert!(files.unwrap_or_default().is_empty());
    }

    #[test]
    fn visit_subdir_skips_missing_and_revisited() {
        let root = temp_root();
        require_ok(fs::write(root.join("lib.rs"), "fn f() {}\n"));
        let mut visited = HashSet::new();
        let mut out = Vec::new();
        let mut walk = Walk {
            root: &root,
            root_canon: None,
            nested_skip: &[],
            visited: &mut visited,
            out: &mut out,
        };
        require_ok(visit_subdir(&root.join("gone"), None, &mut walk));
        assert!(walk.out.is_empty());
        require_ok(visit_subdir(&root, None, &mut walk));
        let before = walk.out.len();
        require_ok(visit_subdir(&root, None, &mut walk));
        let after = walk.out.len();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(after, before);
    }

    #[test]
    fn visit_subdir_skips_outside_root_canon() {
        let root = temp_root();
        let outside = temp_root();
        require_ok(fs::write(outside.join("secret.rs"), "fn leak() {}\n"));
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
        require_ok(visit_subdir(&outside, None, &mut walk));
        let empty = walk.out.is_empty();
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        assert!(empty);
    }
}
