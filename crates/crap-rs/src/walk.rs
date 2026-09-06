//! Collect Rust sources under an analysis root.

use crap_core::{Error, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Directories skipped at any depth.
const SKIP_ALWAYS: &[&str] = &["target", ".git"];

/// Cargo convention dirs skipped only as children of a package root.
const CONVENTION_DIRS: &[&str] = &["tests", "benches", "examples"];

/// Walks `root` for `.rs` files, skipping nested member roots.
///
/// # Errors
///
/// Returns [`Error::Io`] if a directory cannot be read.
pub fn rust_files(root: &Path, nested_skip: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    if let Ok(canon) = fs::canonicalize(root) {
        visited.insert(canon);
    }
    visit(root, root, None, nested_skip, &mut visited, &mut out)?;
    out.sort();
    Ok(out)
}

fn visit(
    dir: &Path,
    walk_root: &Path,
    inherited_pkg: Option<&Path>,
    nested_skip: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let package_root_buf = dir.join("Cargo.toml").is_file().then(|| dir.to_path_buf());
    let package_root = package_root_buf.as_deref().or(inherited_pkg);
    if skip_dir(dir, walk_root, package_root, nested_skip) {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|source| Error::io(dir, source))?;
    walk_entries(
        entries,
        dir,
        walk_root,
        package_root,
        nested_skip,
        visited,
        out,
    )
}

fn walk_entries(
    entries: impl Iterator<Item = std::io::Result<fs::DirEntry>>,
    dir: &Path,
    walk_root: &Path,
    package_root: Option<&Path>,
    nested_skip: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in entries {
        take_entry(
            entry,
            dir,
            walk_root,
            package_root,
            nested_skip,
            visited,
            out,
        )?;
    }
    Ok(())
}

fn take_entry(
    entry: std::io::Result<fs::DirEntry>,
    dir: &Path,
    walk_root: &Path,
    package_root: Option<&Path>,
    nested_skip: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let entry = entry.map_err(|source| Error::io(dir, source))?;
    let path = entry.path();
    let file_type = entry.file_type().map_err(|source| Error::io(&path, source))?;
    if file_type.is_symlink() {
        return take_symlink(&path, walk_root, package_root, nested_skip, visited, out);
    }
    if file_type.is_dir() {
        return visit_subdir(&path, walk_root, package_root, nested_skip, visited, out);
    }
    collect_rust_file(path, visited, out);
    Ok(())
}

fn take_symlink(
    path: &Path,
    walk_root: &Path,
    package_root: Option<&Path>,
    nested_skip: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };
    if meta.is_dir() {
        return visit_subdir(path, walk_root, package_root, nested_skip, visited, out);
    }
    if meta.is_file() {
        collect_rust_file(path.to_path_buf(), visited, out);
    }
    Ok(())
}

fn visit_subdir(
    path: &Path,
    walk_root: &Path,
    package_root: Option<&Path>,
    nested_skip: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let Ok(canon) = fs::canonicalize(path) else {
        return Ok(());
    };
    if !visited.insert(canon) {
        return Ok(());
    }
    visit(path, walk_root, package_root, nested_skip, visited, out)
}

fn collect_rust_file(path: PathBuf, visited: &mut HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
    if !is_rust_file(&path) {
        return;
    }
    if let Ok(canon) = fs::canonicalize(&path)
        && !visited.insert(canon)
    {
        return;
    }
    out.push(path);
}

fn skip_dir(
    dir: &Path,
    walk_root: &Path,
    package_root: Option<&Path>,
    nested_skip: &[PathBuf],
) -> bool {
    if nested_skip.iter().any(|skip| dir == skip) {
        return true;
    }
    if dir == walk_root {
        return false;
    }
    skip_named_dir(dir, package_root)
}

fn skip_named_dir(dir: &Path, package_root: Option<&Path>) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if SKIP_ALWAYS.contains(&name) {
        return true;
    }
    package_root.is_some_and(|pkg| dir.parent() == Some(pkg) && CONVENTION_DIRS.contains(&name))
}

fn is_rust_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "rs")
        && !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_tests.rs"))
}

#[cfg(test)]
mod tests {
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
    fn convention_dirs_skip_only_at_package_root() {
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
                false,
                Path::new("/proj/crates/foo/src/tests"),
                walk,
                Some(pkg),
                empty,
            ),
            (
                false,
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
    fn rust_files_keeps_src_tests_skips_package_tests() {
        let root = temp_root();
        require_ok(fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        ));
        let src_tests = root.join("src/tests");
        require_ok(fs::create_dir_all(&src_tests));
        require_ok(fs::write(src_tests.join("helper.rs"), "fn h() {}\n"));
        require_ok(fs::write(root.join("src/tests.rs"), "fn t() {}\n"));
        let integ_dir = root.join("tests");
        require_ok(fs::create_dir_all(&integ_dir));
        require_ok(fs::write(integ_dir.join("integration.rs"), "fn i() {}\n"));
        let files = require_ok(rust_files(&root, &[]));
        let _ = fs::remove_dir_all(&root);
        let names = ["helper.rs", "tests.rs", "integration.rs"];
        let found = names.map(|name| files.iter().any(|path| path.ends_with(name)));
        assert_eq!(found, [true, true, false]);
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
    fn walk_entries_propagates_read_dir_error() {
        let err = std::io::Error::other("boom");
        let result = walk_entries(
            std::iter::once(Err(err)),
            Path::new("/proj"),
            Path::new("/proj"),
            None,
            &[],
            &mut HashSet::new(),
            &mut Vec::new(),
        );
        assert!(result.is_err());
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "crap-rs-walk-{}-{}",
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
        require_ok(visit_subdir(
            &root.join("gone"),
            &root,
            None,
            &[],
            &mut visited,
            &mut out,
        ));
        assert!(out.is_empty());
        require_ok(visit_subdir(
            &root,
            &root,
            None,
            &[],
            &mut visited,
            &mut out,
        ));
        let before = out.len();
        require_ok(visit_subdir(
            &root,
            &root,
            None,
            &[],
            &mut visited,
            &mut out,
        ));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(out.len(), before);
    }
}
