//! Join tests for LCOV span matching and missing-policy scoring.

use super::*;
use std::path::PathBuf;

fn func(file: &str, name: &str, start: usize, end: usize) -> LocatedFn {
    func_in(file, name, start, end, None)
}

fn func_in(
    file: &str,
    name: &str,
    start: usize,
    end: usize,
    crate_name: Option<&str>,
) -> LocatedFn {
    LocatedFn {
        function: FunctionComplexity {
            file: PathBuf::from(file),
            name: name.into(),
            start_line: start,
            end_line: end,
            complexity: 1,
        },
        crate_name: crate_name.map(str::to_owned),
    }
}

fn cov(path: &str, lines: &[(u32, u64)]) -> HashMap<PathBuf, FileCoverage> {
    let mut map = HashMap::new();
    map.insert(
        PathBuf::from(path),
        FileCoverage {
            lines: lines.iter().copied().collect(),
        },
    );
    map
}

#[test]
fn relative_lcov_suffix_matches_absolute_source() {
    let functions = [func("/proj/src/foo.rs", "f", 10, 12)];
    let coverage = cov("src/foo.rs", &[(10, 1), (11, 0), (12, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries.len(), 1);
    assert!((entries[0].coverage - 2.0 * 100.0 / 3.0).abs() < 1e-9);
}

#[test]
fn longest_suffix_wins() {
    let functions = [func("/proj/src/lib.rs", "f", 1, 1)];
    let mut coverage = cov("src/lib.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("vendor/dep/src/lib.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 100.0);
}

#[test]
fn relative_keys_are_not_resolved_against_cwd() {
    let functions = [func("/other/src/foo.rs", "f", 1, 1)];
    let coverage = cov("src/foo.rs", &[(1, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 100.0);
    assert!(
        !std::env::current_dir()
            .is_ok_and(|cwd| { coverage.contains_key(&cwd.join("src/foo.rs")) })
    );
}

#[test]
fn missing_pessimistic_is_zero() {
    let functions = [func("src/gone.rs", "f", 1, 1)];
    let entries = join(&functions, &HashMap::new(), MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 0.0);
    assert_eq!(entries[0].crap, 2.0);
    assert_eq!(entries[0].start_line, 1);
    assert_eq!(entries[0].end_line, 1);
}

#[test]
fn missing_skip_drops_the_row() {
    let functions = [func("src/gone.rs", "f", 1, 1)];
    let entries = join(&functions, &HashMap::new(), MissingPolicy::Skip);
    assert!(entries.is_empty());
}

#[test]
fn foosrc_does_not_match_src() {
    let functions = [func("/proj/foosrc/lib.rs", "f", 1, 1)];
    let coverage = cov("src/lib.rs", &[(1, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 0.0);
}

#[test]
fn empty_span_is_pessimistic_zero() {
    let functions = [func("src/foo.rs", "f", 10, 12)];
    let coverage = cov("src/foo.rs", &[(1, 1), (20, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 0.0);
    assert_eq!(entries[0].crap, 2.0);
}

#[test]
fn empty_span_skip_drops_the_row() {
    let functions = [func("src/foo.rs", "f", 10, 12)];
    let coverage = cov("src/foo.rs", &[(1, 1), (20, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Skip);
    assert!(entries.is_empty());
}

#[test]
fn empty_span_optimistic_is_full() {
    let functions = [func("src/foo.rs", "f", 10, 12)];
    let coverage = cov("src/foo.rs", &[(1, 1), (20, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Optimistic);
    assert_eq!(entries[0].coverage, 100.0);
}

#[test]
fn parent_dir_resolves_without_merging_unrelated_keys() {
    let functions = [func("/proj/b/src/lib.rs", "f", 1, 2)];
    let mut coverage = cov("a/../b/src/lib.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("a/b/src/lib.rs"),
        FileCoverage {
            lines: std::iter::once((2, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 100.0);
}

#[test]
fn equal_length_crate_suffixes_are_ambiguous() {
    let functions = [func("src/lib.rs", "f", 1, 1)];
    let mut coverage = cov("/crate_a/src/lib.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("/crate_b/src/lib.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 0.0);
}

#[test]
fn crate_name_breaks_equal_length_suffix_tie() {
    let functions = [func_in("src/lib.rs", "f", 1, 1, Some("crate_a"))];
    let mut coverage = cov("/crate_a/src/lib.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("/crate_b/src/lib.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 100.0);
}

#[test]
fn nested_fn_lines_are_excluded_from_outer_coverage() {
    let functions = [
        func("src/foo.rs", "outer", 1, 20),
        func("src/foo.rs", "inner", 5, 12),
    ];
    let coverage = cov(
        "src/foo.rs",
        &[(2, 1), (3, 1), (5, 0), (8, 0), (12, 0), (15, 1), (18, 1)],
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    let outer = entries.iter().find(|e| e.function == "outer");
    let inner = entries.iter().find(|e| e.function == "inner");
    assert!(outer.is_some() && inner.is_some());
    let Some(outer) = outer else {
        return;
    };
    let Some(inner) = inner else {
        return;
    };
    assert_eq!(outer.coverage, 100.0);
    assert_eq!(inner.coverage, 0.0);
    assert_eq!(outer.crap, 1.0);
}

#[test]
fn inner_only_hits_leave_outer_on_missing_policy() {
    let functions = [
        func("src/foo.rs", "outer", 1, 20),
        func("src/foo.rs", "inner", 5, 12),
    ];
    let coverage = cov("src/foo.rs", &[(5, 1), (8, 1), (12, 1)]);
    let pessimistic = join(&functions, &coverage, MissingPolicy::Pessimistic);
    let skip = join(&functions, &coverage, MissingPolicy::Skip);
    let optimistic = join(&functions, &coverage, MissingPolicy::Optimistic);
    let named = |rows: &[CrapEntry], name: &str| {
        rows.iter().find(|e| e.function == name).map(|e| e.coverage)
    };
    assert_eq!(named(&pessimistic, "outer"), Some(0.0));
    assert_eq!(named(&pessimistic, "inner"), Some(100.0));
    assert_eq!(named(&skip, "outer"), None);
    assert_eq!(named(&skip, "inner"), Some(100.0));
    assert_eq!(named(&optimistic, "outer"), Some(100.0));
    assert_eq!(named(&optimistic, "inner"), Some(100.0));
}

#[test]
fn equal_length_crate_suffixes_skip() {
    let functions = [func("src/lib.rs", "f", 1, 1)];
    let mut coverage = cov("/crate_a/src/lib.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("/crate_b/src/lib.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Skip);
    assert!(entries.is_empty());
}

#[test]
fn equal_length_crate_suffixes_optimistic() {
    let functions = [func("src/lib.rs", "f", 1, 1)];
    let mut coverage = cov("/crate_a/src/lib.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("/crate_b/src/lib.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Optimistic);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].coverage, 100.0);
}

#[test]
fn parses_and_displays_missing_policy() {
    assert_eq!(
        "pessimistic".parse::<MissingPolicy>().ok(),
        Some(MissingPolicy::Pessimistic)
    );
    assert_eq!(
        "optimistic".parse::<MissingPolicy>().ok(),
        Some(MissingPolicy::Optimistic)
    );
    assert_eq!(
        "skip".parse::<MissingPolicy>().ok(),
        Some(MissingPolicy::Skip)
    );
    assert!("nope".parse::<MissingPolicy>().is_err());
    assert_eq!(MissingPolicy::Pessimistic.to_string(), "pessimistic");
    assert_eq!(MissingPolicy::Optimistic.to_string(), "optimistic");
    assert_eq!(MissingPolicy::Skip.to_string(), "skip");
}

#[test]
fn leading_parent_dir_stays_on_the_stack() {
    let functions = [func("/src/lib.rs", "f", 1, 1)];
    let coverage = cov("../src/lib.rs", &[(1, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage, 100.0);
}
