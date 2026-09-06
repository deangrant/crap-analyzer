//! Normalize LCOV paths and pick the best suffix match for a source file.

use crate::coverage::FileCoverage;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::BuildHasher;
use std::path::{Component, Path, PathBuf};

/// Coverage files keyed by resolved path components.
pub(super) struct PathIndex {
    files: Vec<(Vec<String>, FileCoverage)>,
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
        Self {
            files: merged.into_iter().collect(),
        }
    }

    pub(super) fn lookup(&self, source: &Path, crate_name: Option<&str>) -> Option<&FileCoverage> {
        let src = components(source);
        let mut best_rank: Option<MatchRank> = None;
        let mut winners: Vec<(&Vec<String>, &FileCoverage)> = Vec::new();
        for (key, file) in &self.files {
            let Some(rank) = match_rank(&src, key) else {
                continue;
            };
            match best_rank {
                Some(best) if rank < best => {}
                Some(best) if rank == best => winners.push((key, file)),
                _ => {
                    best_rank = Some(rank);
                    winners.clear();
                    winners.push((key, file));
                }
            }
        }
        pick_winner(&winners, crate_name)
    }
}

fn pick_winner<'a>(
    winners: &[(&'a Vec<String>, &'a FileCoverage)],
    crate_name: Option<&str>,
) -> Option<&'a FileCoverage> {
    match winners {
        [] => None,
        [(_, file)] => Some(*file),
        many => {
            let name = crate_name?;
            let mut named = many.iter().filter(|(key, _)| key.iter().any(|part| part == name));
            let first = named.next()?;
            if named.next().is_some() {
                return None;
            }
            Some(first.1)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MatchRank {
    len: usize,
    exact: bool,
    src_ends_with_key: bool,
}

fn match_rank(src: &[String], key: &[String]) -> Option<MatchRank> {
    if !is_suffix_pair(src, key) {
        return None;
    }
    Some(MatchRank {
        len: key.len().min(src.len()),
        exact: src == key,
        src_ends_with_key: src.ends_with(key),
    })
}

fn components(path: &Path) -> Vec<String> {
    let mut stack = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Normal(part) => stack.push(os_to_string(part)),
            Component::ParentDir => push_parent(&mut stack),
            #[cfg(windows)]
            Component::Prefix(prefix) => stack.push(os_to_string(prefix.as_os_str())),
            #[cfg(windows)]
            Component::RootDir | Component::CurDir => {}
            #[cfg(not(windows))]
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    stack
}

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
