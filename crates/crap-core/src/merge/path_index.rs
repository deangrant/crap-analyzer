//! Normalize LCOV paths and pick the best suffix match for a source file.

use crate::coverage::FileCoverage;
use std::collections::{HashMap, HashSet};
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

/// Outcome of matching a source path to an LCOV file.
#[derive(Clone, Copy)]
pub(super) enum Lookup<'a> {
    /// Unique best coverage file.
    Found(&'a FileCoverage),
    /// Equal-rank keys that package name could not uniquely break.
    Ambiguous,
    /// No basename or suffix match.
    Missing,
}

impl PathIndex {
    pub(super) fn from_coverage<S: BuildHasher>(
        coverage: &HashMap<PathBuf, FileCoverage, S>,
    ) -> Self {
        // Non-generic body so llvm-cov has one empty-key skip region to cover.
        let mut iter = coverage.iter().map(|(path, file)| (path.as_path(), file));
        Self::from_paths(&mut iter)
    }

    fn from_paths<'a>(coverage: &mut dyn Iterator<Item = (&'a Path, &'a FileCoverage)>) -> Self {
        let mut merged: HashMap<Vec<String>, FileCoverage> = HashMap::new();
        for (path, file) in coverage {
            let key = components(path);
            merged.entry(key).or_default().merge_from(file);
        }
        let mut by_name: HashMap<String, Vec<IndexedFile>> = HashMap::new();
        for (parts, coverage) in merged {
            // Empty keys cannot be looked up by basename.
            if parts.is_empty() {
                continue;
            }
            let name = parts[parts.len() - 1].clone();
            by_name.entry(name).or_default().push(IndexedFile { parts, coverage });
        }
        Self { by_name }
    }

    pub(super) fn lookup(&self, source: &Path, crate_name: Option<&str>) -> Lookup<'_> {
        let src = components(source);
        let Some(name) = src.last() else {
            return Lookup::Missing;
        };
        let Some(files) = self.by_name.get(name) else {
            return Lookup::Missing;
        };
        resolve_winners(
            &best_matches_for_candidates(&src, crate_name, files),
            crate_name,
        )
    }
}

fn best_matches_for_candidates<'a>(
    src: &[String],
    crate_name: Option<&str>,
    files: &'a [IndexedFile],
) -> Vec<(&'a Vec<String>, &'a FileCoverage)> {
    let mut best_rank = None;
    let mut winners = Vec::new();
    for candidate in source_candidates(src, crate_name) {
        merge_candidate_matches(&candidate, files, &mut best_rank, &mut winners);
    }
    winners
}

fn merge_candidate_matches<'a>(
    candidate: &[String],
    files: &'a [IndexedFile],
    best_rank: &mut Option<MatchRank>,
    winners: &mut Vec<(&'a Vec<String>, &'a FileCoverage)>,
) {
    let mut cand_rank = None;
    let mut cand_winners = Vec::new();
    for file in files {
        consider_match(candidate, file, &mut cand_rank, &mut cand_winners);
    }
    let Some(rank) = cand_rank else {
        return;
    };
    if best_rank.is_some_and(|best| rank < best) {
        return;
    }
    if best_rank.is_some_and(|best| rank == best) {
        winners.extend(cand_winners);
        dedupe_winners(winners);
        return;
    }
    *best_rank = Some(rank);
    winners.clear();
    winners.extend(cand_winners);
}

fn dedupe_winners<'a>(winners: &mut Vec<(&'a Vec<String>, &'a FileCoverage)>) {
    let mut seen = HashSet::new();
    winners.retain(|(parts, _)| seen.insert(parts.as_ptr()));
}

fn source_candidates(src: &[String], crate_name: Option<&str>) -> Vec<Vec<String>> {
    let mut out = vec![src.to_vec()];
    // Bare filenames stay unaugmented so `pkg/lib.rs` cannot beat `pkg/src/lib.rs`
    // via a false friend like `src/pkg/lib.rs`; package_hit handles those ties.
    if src.len() <= 1 {
        return out;
    }
    let Some(name) = crate_name else {
        return out;
    };
    let parts = package_parts(name);
    if parts.is_empty() {
        return out;
    }
    out.push(prepend(&parts, src));
    if parts.len() > 1 {
        out.push(prepend(&parts[parts.len() - 1..], src));
    }
    out
}

fn package_parts(name: &str) -> Vec<String> {
    name.split('/').filter(|part| !part.is_empty()).map(str::to_owned).collect()
}

fn prepend(prefix: &[String], src: &[String]) -> Vec<String> {
    let mut out = prefix.to_vec();
    out.extend(src.iter().cloned());
    out
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

fn resolve_winners<'a>(
    winners: &[(&'a Vec<String>, &'a FileCoverage)],
    crate_name: Option<&str>,
) -> Lookup<'a> {
    match winners {
        [] => Lookup::Missing,
        [(_, file)] => Lookup::Found(file),
        many => unique_crate_hit(many, crate_name).map_or(Lookup::Ambiguous, Lookup::Found),
    }
}

fn unique_crate_hit<'a>(
    winners: &[(&'a Vec<String>, &'a FileCoverage)],
    crate_name: Option<&str>,
) -> Option<&'a FileCoverage> {
    let name = crate_name?;
    // Prefer Cargo/TS `{name}/{src|lib|…}` (and Go import suffixes) before a bare
    // path-component hit, so `demo/src/…` wins over false friend `src/demo/…`.
    unique_named(winners, name, true).or_else(|| unique_named(winners, name, false))
}

fn unique_named<'a>(
    winners: &[(&'a Vec<String>, &'a FileCoverage)],
    name: &str,
    strong_only: bool,
) -> Option<&'a FileCoverage> {
    let mut named = winners.iter().filter(|(key, _)| package_hit(key, name, strong_only));
    let first = named.next()?;
    if named.next().is_some() {
        return None;
    }
    Some(first.1)
}

/// Cargo package name, Go import path, or npm package name vs a coverage path key.
fn package_hit(key: &[String], name: &str, strong_only: bool) -> bool {
    let parts: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [] => false,
        [single] if strong_only => crate_root_hit(key, single),
        [single] => crate_root_hit(key, single) || contains_contiguous(key, &[single]),
        multi => import_path_hit(key, multi),
    }
}

fn crate_root_hit(key: &[String], name: &str) -> bool {
    key.windows(2).any(|pair| pair[0] == name && is_source_root(&pair[1]))
}

fn is_source_root(part: &str) -> bool {
    ["src", "lib", "tests", "benches", "examples"].contains(&part)
}

/// Full import path or any non-empty suffix (remapped filesystem keys).
fn import_path_hit(key: &[String], parts: &[&str]) -> bool {
    (1..=parts.len()).rev().any(|len| {
        let suffix = &parts[parts.len() - len..];
        contains_contiguous(key, suffix)
    })
}

fn contains_contiguous(key: &[String], parts: &[&str]) -> bool {
    // `parts` is non-empty: callers only pass suffixes from `1..=parts.len()`.
    key.windows(parts.len())
        .any(|window| window.iter().map(String::as_str).eq(parts.iter().copied()))
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
#[path = "path_index_tests.rs"]
mod tests;
