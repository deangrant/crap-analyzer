//! Collect Rust sources under an analysis root.

use crate::error::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Directories skipped at any depth.
const SKIP_ALWAYS: &[&str] = &["target", ".git"];

/// First-path-component excludes relative to an analysis root.
const DEFAULT_EXCLUDES: &[&str] = &["tests", "benches", "examples"];

/// Walks `root` for `.rs` files, skipping nested member roots.
///
/// # Errors
///
/// Returns [`Error::Io`] if a directory cannot be read.
pub fn rust_files(root: &Path, nested_skip: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    visit(root, root, nested_skip, &mut out)?;
    out.sort();
    Ok(out)
}

fn visit(dir: &Path, root: &Path, nested_skip: &[PathBuf], out: &mut Vec<PathBuf>) -> Result<()> {
    if skip_dir(dir, root, nested_skip) {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|source| Error::io(dir, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::io(dir, source))?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| Error::io(&path, source))?;
        if file_type.is_dir() {
            visit(&path, root, nested_skip, out)?;
            continue;
        }
        if file_type.is_file() && is_rust_file(&path) && !excluded_rel(&path, root) {
            out.push(path);
        }
    }
    Ok(())
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
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .is_some_and(|first| DEFAULT_EXCLUDES.contains(&first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_excludes_match_first_component() {
        let root = Path::new("/proj");
        assert!(excluded_rel(Path::new("/proj/tests/foo.rs"), root));
        assert!(excluded_rel(Path::new("/proj/benches/foo.rs"), root));
        assert!(excluded_rel(Path::new("/proj/examples/foo.rs"), root));
        assert!(!excluded_rel(Path::new("/proj/src/foo.rs"), root));
        assert!(!excluded_rel(Path::new("/proj/src/tests.rs"), root));
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
