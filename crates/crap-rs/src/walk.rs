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
        return Ok(());
    }
    if file_type.is_dir() {
        return visit_subdir(&path, walk_root, package_root, nested_skip, visited, out);
    }
    collect_rust_file(path, out);
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

fn collect_rust_file(path: PathBuf, out: &mut Vec<PathBuf>) {
    if is_rust_file(&path) {
        out.push(path);
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convention_dirs_skip_only_at_package_root() {
        let walk = Path::new("/proj");
        let pkg = Path::new("/proj/crates/foo");
        assert!(skip_dir(
            Path::new("/proj/crates/foo/tests"),
            walk,
            Some(pkg),
            &[]
        ));
        assert!(skip_dir(
            Path::new("/proj/crates/foo/benches"),
            walk,
            Some(pkg),
            &[]
        ));
        assert!(skip_dir(
            Path::new("/proj/crates/foo/examples"),
            walk,
            Some(pkg),
            &[]
        ));
        assert!(!skip_dir(
            Path::new("/proj/crates/foo/src/tests"),
            walk,
            Some(pkg),
            &[]
        ));
        assert!(!skip_dir(
            Path::new("/proj/crates/tests"),
            walk,
            Some(Path::new("/proj")),
            &[]
        ));
        assert!(!skip_dir(pkg, walk, Some(pkg), &[]));
    }

    #[test]
    fn skip_dir_ignores_target_and_nested_members() {
        let root = Path::new("/proj");
        let nested = [PathBuf::from("/proj/inner")];
        assert!(skip_dir(Path::new("/proj/target"), root, None, &nested));
        assert!(skip_dir(Path::new("/proj/inner"), root, None, &nested));
        assert!(!skip_dir(root, root, None, &nested));
        assert!(!skip_dir(Path::new("/proj/src"), root, None, &nested));
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

    #[test]
    fn rust_files_keeps_src_tests_skips_package_tests() {
        let root = temp_root();
        let manifest = fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        );
        assert!(manifest.is_ok(), "{manifest:?}");
        let src_tests = root.join("src/tests");
        let made = fs::create_dir_all(&src_tests);
        assert!(made.is_ok(), "{made:?}");
        let helper = fs::write(src_tests.join("helper.rs"), "fn h() {}\n");
        assert!(helper.is_ok(), "{helper:?}");
        let tests_rs = fs::write(root.join("src/tests.rs"), "fn t() {}\n");
        assert!(tests_rs.is_ok(), "{tests_rs:?}");
        let integ_dir = root.join("tests");
        let made_integ = fs::create_dir_all(&integ_dir);
        assert!(made_integ.is_ok(), "{made_integ:?}");
        let integ = fs::write(integ_dir.join("integration.rs"), "fn i() {}\n");
        assert!(integ.is_ok(), "{integ:?}");
        let files = rust_files(&root, &[]);
        let _ = fs::remove_dir_all(&root);
        assert!(files.is_ok(), "{files:?}");
        let files = files.unwrap_or_default();
        assert!(files.iter().any(|path| path.ends_with("helper.rs")));
        assert!(files.iter().any(|path| path.ends_with("tests.rs")));
        assert!(!files.iter().any(|path| path.ends_with("integration.rs")));
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

    #[test]
    fn directory_symlink_cycle_does_not_hang() {
        let root = temp_root();
        let written = fs::write(root.join("lib.rs"), "fn f() {}\n");
        assert!(written.is_ok(), "{written:?}");
        let loop_dir = root.join("loop");
        let made = fs::create_dir(&loop_dir);
        assert!(made.is_ok(), "{made:?}");
        let linked = symlink(&loop_dir, loop_dir.join("back"));
        assert!(linked.is_ok(), "{linked:?}");
        let files = rust_files(&root, &[]);
        let _ = fs::remove_dir_all(&root);
        assert!(files.is_ok(), "{files:?}");
        let files = files.unwrap_or_default();
        assert!(files.iter().any(|path| path.ends_with("lib.rs")));
    }

    #[test]
    fn rust_file_symlink_is_not_collected() {
        let root = temp_root();
        let written = fs::write(root.join("real.rs"), "fn f() {}\n");
        assert!(written.is_ok(), "{written:?}");
        let linked = symlink(root.join("real.rs"), root.join("alias.rs"));
        assert!(linked.is_ok(), "{linked:?}");
        let files = rust_files(&root, &[]);
        let _ = fs::remove_dir_all(&root);
        assert!(files.is_ok(), "{files:?}");
        let files = files.unwrap_or_default();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("real.rs"));
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
        let written = fs::write(root.join("lib.rs"), "fn f() {}\n");
        assert!(written.is_ok(), "{written:?}");
        let mut visited = HashSet::new();
        let mut out = Vec::new();
        let missing = visit_subdir(&root.join("gone"), &root, None, &[], &mut visited, &mut out);
        assert!(missing.is_ok(), "{missing:?}");
        assert!(out.is_empty());
        let first = visit_subdir(&root, &root, None, &[], &mut visited, &mut out);
        assert!(first.is_ok(), "{first:?}");
        let before = out.len();
        let second = visit_subdir(&root, &root, None, &[], &mut visited, &mut out);
        let _ = fs::remove_dir_all(&root);
        assert!(second.is_ok(), "{second:?}");
        assert_eq!(out.len(), before);
    }
}
