//! Collect Rust sources under an analysis root.

use crate::error::{Error, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Directories skipped at any depth.
const SKIP_ALWAYS: &[&str] = &["target", ".git"];

/// Directory names skipped at any depth relative to an analysis root.
const DEFAULT_EXCLUDES: &[&str] = &["tests", "benches", "examples"];

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
    visit(root, root, nested_skip, &mut visited, &mut out)?;
    out.sort();
    Ok(out)
}

fn visit(
    dir: &Path,
    root: &Path,
    nested_skip: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    if skip_dir(dir, root, nested_skip) {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|source| Error::io(dir, source))?;
    for entry in entries {
        take_entry(entry, dir, root, nested_skip, visited, out)?;
    }
    Ok(())
}

fn take_entry(
    entry: std::io::Result<fs::DirEntry>,
    dir: &Path,
    root: &Path,
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
        return visit_subdir(&path, root, nested_skip, visited, out);
    }
    collect_rust_file(path, root, out);
    Ok(())
}

fn visit_subdir(
    path: &Path,
    root: &Path,
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
    visit(path, root, nested_skip, visited, out)
}

fn collect_rust_file(path: PathBuf, root: &Path, out: &mut Vec<PathBuf>) {
    if is_rust_file(&path) && !excluded_rel(&path, root) {
        out.push(path);
    }
}

fn skip_dir(dir: &Path, root: &Path, nested_skip: &[PathBuf]) -> bool {
    if nested_skip.iter().any(|skip| dir == skip) {
        return true;
    }
    if dir == root {
        return false;
    }
    dir.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| SKIP_ALWAYS.contains(&name))
}

fn is_rust_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "rs")
}

fn excluded_rel(path: &Path, root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    rel.components()
        .any(|c| c.as_os_str().to_str().is_some_and(|name| DEFAULT_EXCLUDES.contains(&name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_excludes_match_any_component() {
        let root = Path::new("/proj");
        assert!(excluded_rel(Path::new("/proj/tests/foo.rs"), root));
        assert!(excluded_rel(Path::new("/proj/benches/foo.rs"), root));
        assert!(excluded_rel(Path::new("/proj/examples/foo.rs"), root));
        assert!(excluded_rel(
            Path::new("/proj/crates/pkg/tests/foo.rs"),
            root
        ));
        assert!(!excluded_rel(Path::new("/proj/src/foo.rs"), root));
        assert!(!excluded_rel(Path::new("/proj/src/tests.rs"), root));
        assert!(!excluded_rel(Path::new("/other/foo.rs"), root));
    }

    #[test]
    fn skip_dir_ignores_target_and_nested_members() {
        let root = Path::new("/proj");
        let nested = [PathBuf::from("/proj/inner")];
        assert!(skip_dir(Path::new("/proj/target"), root, &nested));
        assert!(skip_dir(Path::new("/proj/inner"), root, &nested));
        assert!(!skip_dir(root, root, &nested));
        assert!(!skip_dir(Path::new("/proj/src"), root, &nested));
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cargo-crap-walk-{}-{}",
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
        let missing = visit_subdir(&root.join("gone"), &root, &[], &mut visited, &mut out);
        assert!(missing.is_ok(), "{missing:?}");
        assert!(out.is_empty());
        let first = visit_subdir(&root, &root, &[], &mut visited, &mut out);
        assert!(first.is_ok(), "{first:?}");
        let before = out.len();
        let second = visit_subdir(&root, &root, &[], &mut visited, &mut out);
        let _ = fs::remove_dir_all(&root);
        assert!(second.is_ok(), "{second:?}");
        assert_eq!(out.len(), before);
    }
}
