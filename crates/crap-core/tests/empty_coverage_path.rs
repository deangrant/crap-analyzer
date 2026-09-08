//! Cover `PathIndex` join paths via the published rlib (llvm-cov dual crate).

use crap_core::FileCoverage;
use crap_core::merge::{
    CoverageJoin, FunctionComplexity, LocatedFn, MissingPolicy, ambiguous_join_count, join,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn func(file: &str, crate_name: Option<&str>) -> LocatedFn {
    func_join(file, crate_name, None)
}

fn func_join(file: &str, crate_name: Option<&str>, join_key: Option<&str>) -> LocatedFn {
    LocatedFn {
        function: FunctionComplexity {
            file: PathBuf::from(file),
            name: "f".into(),
            start_line: 1,
            end_line: 1,
            complexity: 1,
        },
        crate_name: crate_name.map(str::to_owned),
        join_key: join_key.map(str::to_owned),
    }
}

fn file(hits: u64) -> FileCoverage {
    FileCoverage {
        lines: std::iter::once((1, hits)).collect(),
    }
}

#[test]
fn empty_coverage_path_components_are_skipped() {
    let coverage = HashMap::from([
        (PathBuf::from("foo/.."), file(1)),
        (PathBuf::from("src/foo.rs"), file(1)),
    ]);
    let entries = join(
        &[func("src/foo.rs", None)],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
    assert!((entries[0].coverage - 100.0).abs() < 1e-9);
}

#[test]
fn equal_length_crate_suffixes_are_ambiguous() {
    let coverage = HashMap::from([
        (PathBuf::from("/crate_a/src/lib.rs"), file(1)),
        (PathBuf::from("/crate_b/src/lib.rs"), file(0)),
    ]);
    let entries = join(
        &[func("src/lib.rs", None)],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Ambiguous);
    assert_eq!(ambiguous_join_count(&entries), 1);
}

#[test]
fn crate_name_breaks_equal_length_suffix_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/crate_a/src/lib.rs"), file(1)),
        (PathBuf::from("/crate_b/src/lib.rs"), file(0)),
    ]);
    let entries = join(
        &[func("src/lib.rs", Some("crate_a"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
    assert!((entries[0].coverage - 100.0).abs() < 1e-9);
}

#[test]
fn import_path_breaks_equal_length_go_suffix_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/mod/pkg_a/foo.go"), file(1)),
        (PathBuf::from("/mod/pkg_b/foo.go"), file(0)),
    ]);
    let entries = join(
        &[func("foo.go", Some("example.com/mod/pkg_a"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
    assert!((entries[0].coverage - 100.0).abs() < 1e-9);
}

#[test]
fn crate_root_layout_breaks_bare_filename_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("demo/src/lib.rs"), file(1)),
        (PathBuf::from("src/demo/lib.rs"), file(0)),
    ]);
    let entries = join(
        &[func("lib.rs", Some("demo"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
    assert!((entries[0].coverage - 100.0).abs() < 1e-9);
}

#[test]
fn package_component_breaks_root_file_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/packages/pkg_a/util.ts"), file(1)),
        (PathBuf::from("/packages/pkg_b/util.ts"), file(0)),
    ]);
    let entries = join(
        &[func("util.ts", Some("pkg_a"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
    assert!((entries[0].coverage - 100.0).abs() < 1e-9);
}

#[test]
fn empty_package_segments_stay_ambiguous() {
    let coverage = HashMap::from([
        (PathBuf::from("/a/src/lib.rs"), file(1)),
        (PathBuf::from("/b/src/lib.rs"), file(0)),
    ]);
    let entries = join(
        &[func("src/lib.rs", Some("///"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Ambiguous);
}

#[test]
fn leading_parent_dir_is_kept_in_coverage_key() {
    let coverage = HashMap::from([(PathBuf::from("../src/foo.rs"), file(1))]);
    let entries = join(
        &[func("../src/foo.rs", None)],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
}

#[test]
fn empty_source_path_is_missing() {
    let coverage = HashMap::from([(PathBuf::from("src/foo.rs"), file(1))]);
    let entries = join(&[func("", None)], &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage_join, CoverageJoin::Missing);
}

#[test]
fn incompatible_paths_same_basename_are_missing() {
    let coverage = HashMap::from([(PathBuf::from("a/foo.rs"), file(1))]);
    let entries = join(
        &[func("b/foo.rs", None)],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Missing);
}

#[test]
fn two_demo_crate_roots_stay_ambiguous() {
    let coverage = HashMap::from([
        (PathBuf::from("/ws/demo/src/lib.rs"), file(1)),
        (PathBuf::from("/other/demo/src/lib.rs"), file(0)),
    ]);
    let entries = join(
        &[func("src/lib.rs", Some("demo"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Ambiguous);
}

#[test]
fn unknown_crate_name_still_joins_raw_match() {
    let coverage = HashMap::from([(PathBuf::from("src/foo.rs"), file(1))]);
    let entries = join(
        &[func("src/foo.rs", Some("nope"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
}

#[test]
fn longer_suffix_outranks_basename_only_key() {
    let coverage = HashMap::from([
        (PathBuf::from("src/foo.rs"), file(1)),
        (PathBuf::from("foo.rs"), file(0)),
    ]);
    let entries = join(
        &[func("proj/src/foo.rs", None)],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
    assert!((entries[0].coverage - 100.0).abs() < 1e-9);
}

#[test]
fn join_key_still_ties_when_absent_from_lcov_paths() {
    // Inverse of scoped join_key break: LCOV keys lack packages/pkg_a.
    let coverage = HashMap::from([
        (PathBuf::from("/vendor/a/src/index.ts"), file(1)),
        (PathBuf::from("/vendor/b/src/index.ts"), file(0)),
    ]);
    let entries = join(
        &[func_join(
            "src/index.ts",
            Some("@scope/a"),
            Some("packages/pkg_a"),
        )],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(entries[0].coverage_join, CoverageJoin::Ambiguous);
    assert!((entries[0].coverage - 0.0).abs() < 1e-9);
}

#[test]
fn many_same_basename_lcov_join_finishes_under_a_second() {
    let mut coverage = HashMap::new();
    for i in 0_u32..80 {
        coverage.insert(
            PathBuf::from(format!("/crate_{i}/src/lib.rs")),
            file(u64::from(i % 2)),
        );
    }
    let functions = [func("src/lib.rs", None)];
    let started = std::time::Instant::now();
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert!(started.elapsed().as_secs() < 1, "{:?}", started.elapsed());
    assert_eq!(entries[0].coverage_join, CoverageJoin::Ambiguous);
    let broken = join(
        &[func("src/lib.rs", Some("crate_3"))],
        &coverage,
        MissingPolicy::Pessimistic,
    );
    assert_eq!(broken[0].coverage_join, CoverageJoin::Measured);
    assert!((broken[0].coverage - 100.0).abs() < 1e-9);
}
