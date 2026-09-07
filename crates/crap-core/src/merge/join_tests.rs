//! Join tests for LCOV span matching and missing-policy scoring.

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

fn func_join(
    file: &str,
    name: &str,
    start: usize,
    end: usize,
    crate_name: Option<&str>,
    join_key: Option<&str>,
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
        join_key: join_key.map(str::to_owned),
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

#[test]
fn nested_fn_path_spellings_still_exclude_inner_span() {
    let functions = [
        func("src/foo.rs", "outer", 1, 20),
        func("./src/foo.rs", "inner", 5, 12),
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
    assert_f64_bits_eq(outer.coverage, 100.0);
    assert_f64_bits_eq(inner.coverage, 0.0);
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
    assert_f64_bits_eq(outer.coverage, 100.0);
    assert_f64_bits_eq(inner.coverage, 0.0);
    assert_f64_bits_eq(outer.crap, 1.0);
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
    let cases = [
        (named(&pessimistic, "outer"), Some(0.0)),
        (named(&pessimistic, "inner"), Some(100.0)),
        (named(&skip, "outer"), None),
        (named(&skip, "inner"), Some(100.0)),
        (named(&optimistic, "outer"), Some(100.0)),
        (named(&optimistic, "inner"), Some(100.0)),
    ];
    for (got, want) in cases {
        assert_eq!(got, want);
    }
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
    assert_f64_bits_eq(entries[0].coverage, 100.0);
    assert_eq!(entries[0].coverage_join, CoverageJoin::Ambiguous);
}

#[test]
fn parses_and_displays_missing_policy() {
    let parsed = [
        ("pessimistic", Some(MissingPolicy::Pessimistic)),
        ("optimistic", Some(MissingPolicy::Optimistic)),
        ("skip", Some(MissingPolicy::Skip)),
        ("nope", None),
    ];
    for (input, expected) in parsed {
        assert_eq!(input.parse::<MissingPolicy>().ok(), expected);
    }
    for policy in [
        MissingPolicy::Pessimistic,
        MissingPolicy::Optimistic,
        MissingPolicy::Skip,
    ] {
        assert_eq!(
            policy.to_string().parse::<MissingPolicy>().ok(),
            Some(policy)
        );
    }
}

#[test]
fn coverage_join_display_matches_as_str() {
    for join in [
        CoverageJoin::Measured,
        CoverageJoin::Missing,
        CoverageJoin::Ambiguous,
    ] {
        assert_eq!(join.to_string(), join.as_str());
    }
}

#[test]
fn leading_parent_dir_stays_on_the_stack() {
    let functions = [func("/src/lib.rs", "f", 1, 1)];
    let coverage = cov("../src/lib.rs", &[(1, 1)]);
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_f64_bits_eq(entries[0].coverage, 100.0);
}

#[test]
fn ts_join_key_breaks_scoped_npm_basename_tie() {
    // Scoped npm names are not path segments; join_key is the package dir.
    let functions = [func_join(
        "src/util.ts",
        "f",
        1,
        1,
        Some("@scope/a"),
        Some("packages/a"),
    )];
    let mut coverage = cov("/repo/packages/a/src/util.ts", &[(1, 1)]);
    coverage.insert(
        PathBuf::from("/repo/packages/b/src/util.ts"),
        FileCoverage {
            lines: std::iter::once((1, 0)).collect(),
        },
    );
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert_eq!(entries[0].coverage_join, CoverageJoin::Measured);
    assert_f64_bits_eq(entries[0].coverage, 100.0);
    assert_eq!(entries[0].crate_name.as_deref(), Some("@scope/a"));
}

#[test]
fn join_hundreds_of_nested_functions_finishes_under_a_second() {
    let mut functions = Vec::with_capacity(300);
    functions.push(func("src/foo.rs", "outer", 1, 2000));
    let mut lines = vec![(1, 1_u64)];
    for i in 0..299 {
        let start = 2 + i * 6;
        let end = start + 4;
        functions.push(func("src/foo.rs", &format!("n{i}"), start, end));
        for line in start..=end {
            let line = u32::try_from(line).unwrap_or(1);
            lines.push((line, 1));
        }
    }
    let coverage = cov("src/foo.rs", &lines);
    let started = std::time::Instant::now();
    let entries = join(&functions, &coverage, MissingPolicy::Pessimistic);
    assert!(started.elapsed().as_secs() < 1, "{:?}", started.elapsed());
    assert_eq!(entries.len(), 300);
}

#[test]
fn merge_ranges_collapses_overlap_and_adjacent() {
    assert_eq!(merge_ranges(vec![]), vec![]);
    assert_eq!(merge_ranges(vec![(3, 5)]), vec![(3, 5)]);
    assert_eq!(
        merge_ranges(vec![(10, 12), (1, 4), (3, 6), (8, 9)]),
        vec![(1, 6), (8, 12)]
    );
}
