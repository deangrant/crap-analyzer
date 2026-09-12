//! Expand npm/pnpm workspace globs (`*`, `**`, exact, `!` exclusions).

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use crap_core::Result;
use crap_core::path_glob;
use std::path::{Path, PathBuf};

/// Collect package directories matching `patterns` under `root`.
///
/// # Errors
///
/// Returns [`crap_core::Error::Io`] when a directory cannot be read.
pub(super) fn matching_package_dirs(root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
    path_glob::matching_package_dirs(root, patterns, is_npm_package, is_skipped_dir)
}

fn is_npm_package(dir: &Path) -> bool {
    dir.join("package.json").is_file()
}

fn is_skipped_dir(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some("node_modules")
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
        let root = Path::new("/tmp/crap-ts-glob-empty-patterns");
        let got = matching_package_dirs(root, &["!".into(), "   ".into()]);
        assert!(got.is_ok(), "{got:?}");
        assert!(got.unwrap_or_default().is_empty());
    }

    #[test]
    fn dir_entry_path_propagates_io_error() {
        let err = dir_entry_path(Path::new("/tmp"), Err(std::io::Error::other("boom")));
        assert!(err.is_err());
    }
}
