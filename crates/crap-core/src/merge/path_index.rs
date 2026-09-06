//! Normalize LCOV paths and pick the best suffix match for a source file.

use crate::coverage::FileCoverage;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::BuildHasher;
use std::path::{Component, Path, PathBuf};

struct IndexedFile {
    parts: Vec<String>,
    coverage: FileCoverage,
}

/// Coverage files keyed by last path component (file name).
pub(super) struct PathIndex {
    by_name: HashMap<String, Vec<IndexedFile>>,
}

impl PathIndex {
    pub(super) fn from_coverage<S: BuildHasher>(
        coverage: &HashMap<PathBuf, FileCoverage, S>,
    ) -> Self {
        let mut merged: HashMap<Vec<String>, FileCoverage> = HashMap::new();
        for (path, file) in coverage {
            let key = components(path);
            merged.entry(key).or_default().merge_from(file);
        }
        let mut by_name: HashMap<String, Vec<IndexedFile>> = HashMap::new();
        for (parts, coverage) in merged {
            if let Some(name) = parts.last().cloned() {
                by_name.entry(name).or_default().push(IndexedFile { parts, coverage });
            }
        }
        Self { by_name }
    }

    pub(super) fn lookup(&self, source: &Path, crate_name: Option<&str>) -> Option<&FileCoverage> {
        let src = components(source);
        let files = self.by_name.get(src.last()?)?;
        pick_winner(&best_matches(&src, files), crate_name)
    }
}

fn best_matches<'a>(
    src: &[String],
    files: &'a [IndexedFile],
) -> Vec<(&'a Vec<String>, &'a FileCoverage)> {
    let mut best_rank = None;
    let mut winners = Vec::new();
    for file in files {
        consider_match(src, file, &mut best_rank, &mut winners);
    }
    winners
}

fn consider_match<'a>(
    src: &[String],
    file: &'a IndexedFile,
    best_rank: &mut Option<MatchRank>,
    winners: &mut Vec<(&'a Vec<String>, &'a FileCoverage)>,
) {
    let Some(rank) = match_rank(src, &file.parts) else {
        return;
    };
    apply_rank(rank, file, best_rank, winners);
}

fn apply_rank<'a>(
    rank: MatchRank,
    file: &'a IndexedFile,
    best_rank: &mut Option<MatchRank>,
    winners: &mut Vec<(&'a Vec<String>, &'a FileCoverage)>,
) {
    if best_rank.is_some_and(|best| rank < best) {
        return;
    }
    if best_rank.is_some_and(|best| rank == best) {
        winners.push((&file.parts, &file.coverage));
        return;
    }
    *best_rank = Some(rank);
    winners.clear();
    winners.push((&file.parts, &file.coverage));
}

fn pick_winner<'a>(
    winners: &[(&'a Vec<String>, &'a FileCoverage)],
    crate_name: Option<&str>,
) -> Option<&'a FileCoverage> {
    match winners {
        [] => None,
        [(_, file)] => Some(*file),
        many => unique_crate_hit(many, crate_name),
    }
}

fn unique_crate_hit<'a>(
    winners: &[(&'a Vec<String>, &'a FileCoverage)],
    crate_name: Option<&str>,
) -> Option<&'a FileCoverage> {
    let name = crate_name?;
    let mut named = winners.iter().filter(|(key, _)| crate_root_hit(key, name));
    let first = named.next()?;
    if named.next().is_some() {
        return None;
    }
    Some(first.1)
}

fn crate_root_hit(key: &[String], name: &str) -> bool {
    key.windows(2).any(|pair| pair[0] == name && is_source_root(&pair[1]))
}

fn is_source_root(part: &str) -> bool {
    ["src", "tests", "benches", "examples"].contains(&part)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MatchRank {
    src_ends_with_key: bool,
    len: usize,
    exact: bool,
}

fn match_rank(src: &[String], key: &[String]) -> Option<MatchRank> {
    if !is_suffix_pair(src, key) {
        return None;
    }
    Some(MatchRank {
        src_ends_with_key: src.ends_with(key),
        len: key.len().min(src.len()),
        exact: src == key,
    })
}

pub(super) fn components(path: &Path) -> Vec<String> {
    let mut stack = Vec::new();
    for comp in path.components() {
        push_component(&mut stack, comp);
    }
    stack
}

fn push_component(stack: &mut Vec<String>, comp: Component<'_>) {
    match comp {
        Component::Normal(part) => stack.push(os_to_string(part)),
        Component::ParentDir => push_parent(stack),
        other => push_prefix(stack, other),
    }
}

#[cfg(windows)]
fn push_prefix(stack: &mut Vec<String>, comp: Component<'_>) {
    if let Component::Prefix(prefix) = comp {
        stack.push(os_to_string(prefix.as_os_str()));
    }
}

#[cfg(not(windows))]
const fn push_prefix(_stack: &mut Vec<String>, _comp: Component<'_>) {}

fn push_parent(stack: &mut Vec<String>) {
    if stack.last().is_some_and(|top| top != "..") {
        stack.pop();
    } else {
        stack.push("..".into());
    }
}

fn os_to_string(part: &OsStr) -> String {
    part.to_string_lossy().replace('\\', "/")
}

fn is_suffix_pair(a: &[String], b: &[String]) -> bool {
    a.ends_with(b) || b.ends_with(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(hits: u64) -> FileCoverage {
        FileCoverage {
            lines: std::iter::once((1, hits)).collect(),
        }
    }

    #[test]
    fn lookup_only_considers_the_same_basename() {
        let coverage = HashMap::from([
            (PathBuf::from("src/foo.rs"), file(1)),
            (PathBuf::from("src/bar.rs"), file(99)),
        ]);
        let index = PathIndex::from_coverage(&coverage);
        let found = index.lookup(Path::new("/proj/src/foo.rs"), None);
        assert_eq!(found.and_then(|cov| cov.lines.get(&1).copied()), Some(1));
        assert!(index.lookup(Path::new("/proj/src/missing.rs"), None).is_none());
        assert!(index.lookup(Path::new(""), None).is_none());
    }

    #[test]
    fn two_crate_named_suffixes_are_ambiguous() {
        let coverage = HashMap::from([
            (PathBuf::from("/ws/demo/src/lib.rs"), file(1)),
            (PathBuf::from("/other/demo/src/lib.rs"), file(99)),
        ]);
        let index = PathIndex::from_coverage(&coverage);
        assert!(index.lookup(Path::new("src/lib.rs"), Some("demo")).is_none());
    }

    #[test]
    fn apply_rank_ignores_a_worse_suffix() {
        let better = IndexedFile {
            parts: vec!["src".into(), "foo.rs".into()],
            coverage: file(1),
        };
        let worse = IndexedFile {
            parts: vec!["foo.rs".into()],
            coverage: file(9),
        };
        let src = vec!["proj".into(), "src".into(), "foo.rs".into()];
        let mut best_rank = None;
        let mut winners = Vec::new();
        consider_match(&src, &better, &mut best_rank, &mut winners);
        consider_match(&src, &worse, &mut best_rank, &mut winners);
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].1.lines.get(&1).copied(), Some(1));
    }
}
