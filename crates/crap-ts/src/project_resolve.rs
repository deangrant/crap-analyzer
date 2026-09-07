//! Resolve npm packages from `package.json` and the filesystem.

use crap_core::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// A package directory discovered from `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Package `name` used as the report package key.
    pub name: String,
    /// Directory that contains the package's sources.
    pub root: PathBuf,
}

/// Loads every package under the workspace or single package at `root`.
///
/// When `package.json` lists `workspaces`, expands those globs. Otherwise
/// returns the single package at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if `package.json` is missing or malformed, or
/// [`Error::Io`] if directories cannot be read.
pub fn all_packages(root: &Path) -> Result<Vec<Package>> {
    let text = read_package_json(root)?;
    let patterns = workspace_patterns(&text);
    if patterns.is_empty() {
        return Ok(vec![package_from_json(root, &text)]);
    }
    discover_workspace_packages(root, &patterns)
}

/// Loads only the package defined by `package.json` at `root` (no workspace expand).
///
/// # Errors
///
/// Returns [`Error::Io`] or [`Error::Resolve`] when `package.json` cannot be read.
pub fn root_package(root: &Path) -> Result<Package> {
    let text = read_package_json(root)?;
    Ok(package_from_json(root, &text))
}

/// Loads the named packages under the workspace at `root`.
///
/// # Errors
///
/// Returns [`Error::Resolve`] if a name is missing or `package.json` is invalid.
pub fn selected_packages(names: &[String], root: &Path) -> Result<Vec<Package>> {
    let packages = all_packages(root)?;
    let mut selected = Vec::with_capacity(names.len());
    for name in names {
        let Some(pkg) = packages.iter().find(|p| p.name == *name) else {
            return Err(Error::resolve(format!("unknown package `{name}`")));
        };
        selected.push(pkg.clone());
    }
    Ok(selected)
}

/// Nested package roots under `root` that a walk must not enter.
#[must_use]
pub fn nested_package_roots(root: &Path, all: &[Package]) -> Vec<PathBuf> {
    all.iter()
        .map(|pkg| pkg.root.as_path())
        .filter(|other| *other != root && other.starts_with(root))
        .map(Path::to_path_buf)
        .collect()
}

fn read_package_json(root: &Path) -> Result<String> {
    let path = root.join("package.json");
    fs::read_to_string(&path).map_err(|source| Error::io(&path, source))
}

fn package_from_json(root: &Path, text: &str) -> Package {
    Package {
        name: package_name(text).unwrap_or_else(|| fallback_name(root)),
        root: root.to_path_buf(),
    }
}

fn fallback_name(root: &Path) -> String {
    root.file_name().and_then(|n| n.to_str()).unwrap_or("package").to_owned()
}

fn discover_workspace_packages(root: &Path, patterns: &[String]) -> Result<Vec<Package>> {
    let mut packages = Vec::new();
    for pattern in patterns {
        expand_pattern(root, pattern, &mut packages)?;
    }
    if packages.is_empty() {
        return Err(Error::resolve(format!(
            "{}: workspace patterns matched no packages",
            root.display()
        )));
    }
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

fn expand_pattern(root: &Path, pattern: &str, out: &mut Vec<Package>) -> Result<()> {
    let Some((parent, leaf)) = split_glob(pattern) else {
        return take_package_dir(&root.join(pattern), out);
    };
    let dir = if parent.is_empty() {
        root.to_path_buf()
    } else {
        root.join(parent)
    };
    if leaf == "*" {
        return expand_star(&dir, out);
    }
    take_package_dir(&dir.join(leaf), out)
}

fn split_glob(pattern: &str) -> Option<(&str, &str)> {
    let normalized = pattern.trim_start_matches("./");
    let (parent, leaf) = normalized.rsplit_once('/')?;
    Some((parent, leaf))
}

fn expand_star(dir: &Path, out: &mut Vec<Package>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries = read_dir_entries(dir)?;
    for entry in entries {
        take_star_entry(dir, entry, out)?;
    }
    Ok(())
}

fn read_dir_entries(dir: &Path) -> Result<fs::ReadDir> {
    fs::read_dir(dir).map_err(|source| Error::io(dir, source))
}

fn take_star_entry(
    dir: &Path,
    entry: std::io::Result<fs::DirEntry>,
    out: &mut Vec<Package>,
) -> Result<()> {
    let entry = entry.map_err(|source| Error::io(dir, source))?;
    let path = entry.path();
    if path.is_dir() {
        take_package_dir(&path, out)?;
    }
    Ok(())
}

fn take_package_dir(path: &Path, out: &mut Vec<Package>) -> Result<()> {
    if !path.join("package.json").is_file() {
        return Ok(());
    }
    let text = read_package_json(path)?;
    out.push(package_from_json(path, &text));
    Ok(())
}

fn package_name(text: &str) -> Option<String> {
    json_string_field(text, "name")
}

fn workspace_patterns(text: &str) -> Vec<String> {
    if let Some(patterns) = json_string_array_field(text, "workspaces") {
        return patterns;
    }
    json_nested_packages_array(text).unwrap_or_default()
}

fn json_string_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after_key = text.find(&needle)? + needle.len();
    let after_colon = skip_ws_and_colon(&text[after_key..])?;
    parse_json_string(&text[after_key + after_colon..])
}

fn json_string_array_field(text: &str, key: &str) -> Option<Vec<String>> {
    let needle = format!("\"{key}\"");
    let after_key = text.find(&needle)? + needle.len();
    let after_colon = skip_ws_and_colon(&text[after_key..])?;
    let rest = &text[after_key + after_colon..];
    if rest.trim_start().starts_with('[') {
        return parse_json_string_array(rest);
    }
    None
}

fn json_nested_packages_array(text: &str) -> Option<Vec<String>> {
    let workspaces = text.find("\"workspaces\"")?;
    let after = &text[workspaces..];
    let packages = after.find("\"packages\"")?;
    let after_key = packages + "\"packages\"".len();
    let after_colon = skip_ws_and_colon(&after[after_key..])?;
    parse_json_string_array(&after[after_key + after_colon..])
}

fn skip_ws_and_colon(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let i = skip_ws_bytes(bytes, 0);
    if bytes.get(i) != Some(&b':') {
        return None;
    }
    Some(skip_ws_bytes(bytes, i + 1))
}

fn skip_ws_bytes(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn parse_json_string(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    collect_json_string(bytes)
}

fn collect_json_string(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        match take_json_char(bytes, i, &mut out)? {
            JsonChar::Done => return Some(out),
            JsonChar::Next(next) => i = next,
        }
    }
    None
}

enum JsonChar {
    Done,
    Next(usize),
}

fn take_json_char(bytes: &[u8], i: usize, out: &mut String) -> Option<JsonChar> {
    let b = bytes[i];
    if b == b'"' {
        return Some(JsonChar::Done);
    }
    if b == b'\\' {
        let ch = *bytes.get(i + 1)? as char;
        out.push(ch);
        return Some(JsonChar::Next(i + 2));
    }
    out.push(b as char);
    Some(JsonChar::Next(i + 1))
}

fn parse_json_string_array(text: &str) -> Option<Vec<String>> {
    let mut i = after_array_open(text)?;
    let mut out = Vec::new();
    loop {
        i = skip_ws_and_commas(text, i);
        if closed_bracket(text, i) {
            return Some(out);
        }
        let (value, next) = read_array_string(text, i)?;
        out.push(value);
        i = next;
    }
}

fn after_array_open(text: &str) -> Option<usize> {
    let i = skip_ws(text, 0);
    if text.as_bytes().get(i) != Some(&b'[') {
        return None;
    }
    Some(i + 1)
}

fn closed_bracket(text: &str, i: usize) -> bool {
    text.as_bytes().get(i) == Some(&b']')
}

fn read_array_string(text: &str, i: usize) -> Option<(String, usize)> {
    let value = parse_json_string(&text[i..])?;
    let next = after_json_string(text, i)?;
    Some((value, next))
}

fn skip_ws(text: &str, mut i: usize) -> usize {
    let bytes = text.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn skip_ws_and_commas(text: &str, mut i: usize) -> usize {
    let bytes = text.as_bytes();
    while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
        i += 1;
    }
    i
}

fn after_json_string(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    scan_json_string_end(bytes, start + 1)
}

fn scan_json_string_end(bytes: &[u8], mut i: usize) -> Option<usize> {
    while i < bytes.len() {
        match after_json_byte(bytes, i) {
            0 => return Some(i + 1),
            step => i += step,
        }
    }
    None
}

fn after_json_byte(bytes: &[u8], i: usize) -> usize {
    match bytes[i] {
        b'"' => 0,
        b'\\' => 2,
        _ => 1,
    }
}

#[cfg(test)]
#[path = "project_resolve_tests.rs"]
mod tests;
