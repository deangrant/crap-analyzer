//! Minimal `pnpm-workspace.yaml` `packages:` list reader.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use crap_core::{Error, Result};
use std::fs;
use std::path::Path;

/// Reads workspace patterns from `pnpm-workspace.yaml` at `root`, if present.
///
/// Returns `Ok(None)` when the file does not exist. Supports a simple
/// `packages:` string list (quoted or bare entries, `#` comments).
///
/// # Errors
///
/// Returns [`Error::Io`] on read failure, or [`Error::Resolve`] when the file
/// exists but has no usable `packages:` list.
pub(super) fn pnpm_workspace_patterns(root: &Path) -> Result<Option<Vec<String>>> {
    let path = root.join("pnpm-workspace.yaml");
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_workspace_yaml(&path)?;
    packages_from_yaml_text(&path, &text)
}

fn read_workspace_yaml(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(source) => Err(Error::io(path, source)),
    }
}

fn packages_from_yaml_text(path: &Path, text: &str) -> Result<Option<Vec<String>>> {
    // Prefer `match` over `map_or_else` so llvm-cov does not keep an unused
    // closure instantiation that leaves a phantom uncovered line.
    #[expect(
        clippy::option_if_let_else,
        reason = "match avoids uncovered map_or_else closure CGUs"
    )]
    match parse_packages_list(text) {
        Some(patterns) => Ok(Some(patterns)),
        None => Err(Error::resolve(format!(
            "{}: missing or invalid packages list",
            path.display()
        ))),
    }
}

fn parse_packages_list(text: &str) -> Option<Vec<String>> {
    let mut in_packages = false;
    let mut out = Vec::new();
    for raw in text.lines() {
        if !consume_packages_line(raw, &mut in_packages, &mut out)? {
            break;
        }
    }
    in_packages.then_some(out)
}

/// Returns `Some(false)` to stop the scan, `Some(true)` to continue, `None` on error.
fn consume_packages_line(raw: &str, in_packages: &mut bool, out: &mut Vec<String>) -> Option<bool> {
    let line = strip_yaml_comment(raw);
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Some(true);
    }
    if !*in_packages {
        if trimmed == "packages:" {
            *in_packages = true;
        }
        return Some(true);
    }
    packages_body_line(trimmed, out)
}

fn packages_body_line(trimmed: &str, out: &mut Vec<String>) -> Option<bool> {
    if let Some(item) = list_item(trimmed) {
        out.push(item);
        return Some(true);
    }
    if trimmed.starts_with('-') {
        return Some(true);
    }
    if looks_like_yaml_key(trimmed) {
        return Some(false);
    }
    None
}

fn list_item(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix('-')?.trim();
    if rest.is_empty() {
        return None;
    }
    Some(unquote(rest))
}

fn looks_like_yaml_key(trimmed: &str) -> bool {
    !trimmed.starts_with('-') && trimmed.ends_with(':') && !trimmed.contains(' ')
}

fn strip_yaml_comment(line: &str) -> &str {
    let end = find_yaml_comment_index(line).unwrap_or(line.len());
    &line[..end]
}

fn find_yaml_comment_index(line: &str) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;
    for (i, ch) in line.char_indices() {
        if yaml_comment_at(ch, &mut in_single, &mut in_double) {
            return Some(i);
        }
    }
    None
}

fn yaml_comment_at(ch: char, in_single: &mut bool, in_double: &mut bool) -> bool {
    if apply_yaml_quote(ch, in_single, in_double) {
        return false;
    }
    ch == '#' && !*in_single && !*in_double
}

#[inline(never)]
fn apply_yaml_quote(ch: char, in_single: &mut bool, in_double: &mut bool) -> bool {
    let is_single_quote = ch == '\'';
    let is_double_quote = ch == '"';
    let free_for_single = !*in_double;
    let free_for_double = !*in_single;
    // Indexing avoids `&&` short-circuit holes under llvm-cov.
    let single = [false, free_for_single][usize::from(is_single_quote)];
    let double = [false, free_for_double][usize::from(is_double_quote)];
    *in_single ^= single;
    *in_double ^= double;
    single || double
}

fn unquote(value: &str) -> String {
    strip_matching_quotes(value).map_or_else(|| value.to_owned(), str::to_owned)
}

fn strip_matching_quotes(value: &str) -> Option<&str> {
    if value.len() < 2 {
        return None;
    }
    let bytes = value.as_bytes();
    let quote = bytes[0];
    if !is_quote(quote) || bytes[value.len() - 1] != quote {
        return None;
    }
    Some(&value[1..value.len() - 1])
}

const fn is_quote(byte: u8) -> bool {
    byte == b'\'' || byte == b'"'
}

#[cfg(test)]
mod tests {
    use super::{parse_packages_list, pnpm_workspace_patterns, strip_yaml_comment, unquote};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_temp(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("crap-ts-pnpm-{label}-{n}"))
    }

    #[test]
    fn parses_quoted_and_bare_entries() {
        let text = "\
# header
packages:

  - 'packages/*'
  - apps/**
  - \"!**/test\"
  - 

catalog:
  foo: 1
";
        let got = parse_packages_list(text);
        assert!(got.is_some(), "{got:?}");
        assert_eq!(
            got.unwrap_or_default(),
            vec!["packages/*", "apps/**", "!**/test"]
        );
    }

    #[test]
    fn invalid_packages_entry_returns_none() {
        let text = "\
packages:
  - ok
  not-a-list-item
";
        assert!(parse_packages_list(text).is_none());
    }

    #[test]
    fn comment_inside_quotes_is_kept() {
        assert_eq!(strip_yaml_comment("  - 'a#b'"), "  - 'a#b'");
        assert_eq!(strip_yaml_comment("  - a # note"), "  - a ");
    }

    #[test]
    fn unquote_strips_matching_quotes() {
        assert_eq!(unquote("'x'"), "x");
        assert_eq!(unquote("\"y\""), "y");
        assert_eq!(unquote("z"), "z");
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_pnpm_workspace_yaml_is_io_error() {
        use std::os::unix::fs::PermissionsExt;

        let root = unique_temp("unreadable");
        assert!(fs::create_dir_all(&root).is_ok());
        let yaml = root.join("pnpm-workspace.yaml");
        assert!(fs::write(&yaml, "packages:\n  - 'a'\n").is_ok());
        assert!(fs::set_permissions(&yaml, fs::Permissions::from_mode(0o000)).is_ok());
        let result = pnpm_workspace_patterns(&root);
        let _ = fs::set_permissions(&yaml, fs::Permissions::from_mode(0o644));
        let _ = fs::remove_dir_all(&root);
        assert!(result.is_err());
    }
}
