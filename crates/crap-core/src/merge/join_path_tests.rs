//! Join tests for LCOV path ranking and package-name tie-breaks.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::*;
use crate::score::assert_f64_bits_eq;
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
        join_key: None,
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
fn forward_relative_sf_beats_longer_reverse_false_friend() {
    let functions = [func("proj/src/foo.rs", "f", 1, 1)];
    let mut coverage = cov("src/foo.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("/unrelated/proj/src/foo.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 100.0);
}

#[test]
fn relative_sf_beats_other_proj_reverse_false_friend() {
    let functions = [func("proj/src/foo.rs", "f", 1, 1)];
    let mut coverage = cov("src/foo.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("other/proj/src/foo.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 100.0);
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
    assert_f64_bits_eq(entries[0].coverage, 100.0);
}

#[test]
fn relative_keys_are_not_resolved_against_cwd() {
    let functions = [func("/other/src/foo.rs", "f", 1, 1)];
    let coverage = cov("src/foo.rs", &[(1, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 100.0);
    assert!(
        !std::env::current_dir()
            .is_ok_and(|cwd| { coverage.contains_key(&cwd.join("src/foo.rs")) })
    );
}

#[test]
fn missing_pessimistic_is_zero() {
    let functions = [func("src/gone.rs", "f", 1, 1)];
    let entries = join(&functions, &HashMap::new(), MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 0.0);
    assert_f64_bits_eq(entries[0].crap, 2.0);
    assert_eq!(entries[0].coverage_join, CoverageJoin::Missing);
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
    assert_f64_bits_eq(entries[0].coverage, 0.0);
}

#[test]
fn empty_span_is_pessimistic_zero() {
    let functions = [func("src/foo.rs", "f", 10, 12)];
    let coverage = cov("src/foo.rs", &[(1, 1), (20, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 0.0);
    assert_f64_bits_eq(entries[0].crap, 2.0);
    assert_eq!(entries[0].coverage_join, CoverageJoin::Missing);
}

#[test]
fn empty_span_missing_path_and_ambiguous_contrast() {
    // Empty span and missing SF both label Missing today; Ambiguous is distinct.
    let empty = join(
        &[func("src/foo.rs", "empty", 10, 12)],
        &cov("src/foo.rs", &[(1, 1), (20, 1)]),
        MissingPolicy::Pessimistic,
    );
    let missing = join(
        &[func("src/gone.rs", "gone", 1, 1)],
        &HashMap::new(),
        MissingPolicy::Pessimistic,
    );
    let mut tie = cov("/crate_a/src/lib.rs", &[(1, 1)]);
    tie.insert(
        PathBuf::from("/crate_b/src/lib.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let ambiguous = join(
        &[func("src/lib.rs", "tie", 1, 1)],
        &tie,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(empty[0].coverage_join, CoverageJoin::Missing);
    assert_f64_bits_eq(empty[0].coverage, 0.0);
    assert_eq!(missing[0].coverage_join, CoverageJoin::Missing);
    assert_f64_bits_eq(missing[0].coverage, 0.0);
    assert_eq!(ambiguous[0].coverage_join, CoverageJoin::Ambiguous);
    assert_f64_bits_eq(ambiguous[0].coverage, 0.0);
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
    assert_f64_bits_eq(entries[0].coverage, 100.0);
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
    assert_f64_bits_eq(entries[0].coverage, 100.0);
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
    assert_f64_bits_eq(entries[0].coverage, 0.0);
    assert_eq!(entries[0].coverage_join, CoverageJoin::Ambiguous);
    assert_eq!(ambiguous_join_count(&entries), 1);
    assert!(ambiguous_join_warning(&entries).contains("ambiguous coverage paths"));
}

#[test]
fn crate_name_requires_package_src_dir() {
    let functions = [func_in("lib.rs", "f", 1, 1, Some("demo"))];
    let mut coverage = cov("demo/src/lib.rs", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("src/demo/lib.rs"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 100.0);
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
    assert_f64_bits_eq(entries[0].coverage, 100.0);
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
}

#[test]
fn import_path_breaks_equal_length_go_suffix_tie() {
    let functions = [func_in("foo.go", "f", 1, 1, Some("example.com/mod/pkg_a"))];
    let mut coverage = cov("/mod/pkg_a/foo.go", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("/mod/pkg_b/foo.go"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 100.0);
}
