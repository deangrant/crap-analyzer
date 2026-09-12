//! Expand uv workspace globs (`*`, `**`, exact, `!` exclusions).

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use crap_core::path_glob;
use crap_core::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Collect project directories matching `patterns` under `root`.
///
/// # Errors
///
/// Returns [`crap_core::Error::Io`] when a directory cannot be read.
pub(super) fn matching_package_dirs(root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
    path_glob::matching_package_dirs(root, patterns, is_python_project, is_skipped_dir)
}

/// Find every directory under `root` (inclusive) that contains `pyproject.toml`.
///
/// # Errors
///
/// Returns [`crap_core::Error::Io`] when a directory cannot be read.
pub(super) fn discover_nested_package_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if is_python_project(root) {
        found.push(root.to_path_buf());
    }
    walk_for_projects(root, &mut found)?;
    found.sort();
    found.dedup();
    Ok(found)
}

fn walk_for_projects(dir: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|source| Error::io(dir, source))?;
    for entry in entries {
        let path = path_glob::dir_entry_path(dir, entry)?;
        if !path.is_dir() || is_skipped_dir(&path) {
            continue;
        }
        if is_python_project(&path) {
            found.push(path.clone());
        }
        walk_for_projects(&path, found)?;
    }
    Ok(())
}

fn is_python_project(dir: &Path) -> bool {
    dir.join("pyproject.toml").is_file()
}

fn is_skipped_dir(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|name| {
        matches!(
            name,
            ".venv"
                | "venv"
                | "__pycache__"
                | ".git"
                | "dist"
                | "build"
                | ".tox"
                | "htmlcov"
                | "coverage"
                | ".eggs"
        ) || name.ends_with(".egg-info")
    })
}

#[cfg(test)]
mod tests {
    use super::matching_package_dirs;
    use crap_core::path_glob::{dir_entry_path, is_excluded, normalize_pattern, path_matches_glob};
    use std::path::Path;

    #[test]
    fn match_segments_star_and_literal() {
        assert!(path_matches_glob("packages/a", "packages/*"));
        assert!(path_matches_glob("packages/a/b", "packages/**"));
        assert!(!path_matches_glob("packages/a", "apps/*"));
    }

    #[test]
    fn normalize_strips_dot_slash_and_keeps_bang() {
        assert_eq!(normalize_pattern("./packages/*"), "packages/*");
        assert_eq!(normalize_pattern("!./packages/skip"), "!packages/skip");
    }

    #[test]
    fn exclude_ignores_paths_outside_root() {
        assert!(!is_excluded(
            Path::new("/workspace"),
            Path::new("/other/pkg"),
            &["packages/skip".into()],
        ));
    }

    #[test]
    fn empty_and_bang_only_patterns_are_skipped() {
        let root = Path::new("/tmp/crap-py-glob-empty-patterns");
        let got = matching_package_dirs(root, &["!".into(), "   ".into()]);
        assert!(got.is_ok(), "{got:?}");
        assert!(got.unwrap_or_default().is_empty());
    }

    #[test]
    fn dir_entry_path_propagates_io_error() {
        let err = dir_entry_path(Path::new("/tmp"), Err(std::io::Error::other("boom")));
        assert!(err.is_err());
    }

    #[test]
    fn discover_nested_skips_egg_info_dirs() {
        use super::discover_nested_package_dirs;
        use std::fs;
        use std::sync::atomic::{AtomicU64, Ordering};

        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("crap-py-nested-egg-{n}"));
        let _ = fs::create_dir_all(root.join("pkg.egg-info"));
        assert!(fs::write(root.join("pyproject.toml"), "[project]\nname = \"root\"\n").is_ok());
        assert!(
            fs::write(
                root.join("pkg.egg-info/pyproject.toml"),
                "[project]\nname = \"hidden\"\n"
            )
            .is_ok()
        );
        let dirs = discover_nested_package_dirs(&root);
        assert!(dirs.is_ok(), "{dirs:?}");
        let dirs = dirs.unwrap_or_default();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0], root);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_nested_without_root_manifest() {
        use super::discover_nested_package_dirs;
        use std::fs;
        use std::sync::atomic::{AtomicU64, Ordering};

        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("crap-py-nested-noroot-{n}"));
        assert!(fs::create_dir_all(root.join("packages/a")).is_ok());
        assert!(
            fs::write(
                root.join("packages/a/pyproject.toml"),
                "[project]\nname = \"a\"\n"
            )
            .is_ok()
        );
        let dirs = discover_nested_package_dirs(&root);
        assert!(dirs.is_ok(), "{dirs:?}");
        let dirs = dirs.unwrap_or_default();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with("packages/a"));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn discover_nested_unreadable_subdir_is_io_error() {
        use super::discover_nested_package_dirs;
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};

        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("crap-py-nested-unreadable-{n}"));
        let locked = root.join("locked");
        assert!(fs::create_dir_all(&locked).is_ok());
        assert!(fs::write(root.join("pyproject.toml"), "[project]\nname = \"root\"\n").is_ok());
        assert!(fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).is_ok());
        let result = discover_nested_package_dirs(&root);
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&root);
        assert!(result.is_err(), "{result:?}");
    }
}
