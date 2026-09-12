//! Path index lookup and package-hit tests.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::*;

fn file(hits: u64) -> FileCoverage {
    FileCoverage {
        lines: std::iter::once((1, hits)).collect(),
    }
}

fn hit_line(lookup: Lookup<'_>) -> Option<u64> {
    match lookup {
        Lookup::Found(cov) => cov.lines.get(&1).copied(),
        Lookup::Ambiguous | Lookup::Missing => None,
    }
}

#[test]
fn hit_line_is_none_for_ambiguous_and_missing() {
    assert_eq!(hit_line(Lookup::Ambiguous), None);
    assert_eq!(hit_line(Lookup::Missing), None);
}

#[test]
fn dedupe_winners_skips_duplicate_parts() {
    let only = IndexedFile {
        parts: vec!["src".into(), "foo.rs".into()],
        coverage: file(1),
    };
    let other = IndexedFile {
        parts: vec!["other".into(), "foo.rs".into()],
        coverage: file(9),
    };
    let mut winners = vec![
        (&only.parts, &only.coverage),
        (&only.parts, &only.coverage),
        (&other.parts, &other.coverage),
    ];
    dedupe_winners(&mut winners);
    assert_eq!(winners.len(), 2);
    assert_eq!(winners[0].1.lines.get(&1).copied(), Some(1));
    assert_eq!(winners[1].1.lines.get(&1).copied(), Some(9));
}

#[test]
fn slash_only_package_name_does_not_augment() {
    let coverage = HashMap::from([(PathBuf::from("src/foo.rs"), file(1))]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("src/foo.rs"), Some("///"))),
        Some(1)
    );
}

#[test]
fn empty_coverage_path_components_are_skipped() {
    let coverage = HashMap::from([
        (PathBuf::from("foo/.."), file(1)),
        (PathBuf::from("src/foo.rs"), file(2)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("src/foo.rs"), None)),
        Some(2)
    );
}

#[test]
fn empty_package_segments_cannot_break_a_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/a/src/lib.rs"), file(1)),
        (PathBuf::from("/b/src/lib.rs"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert!(matches!(
        index.lookup(Path::new("src/lib.rs"), Some("///")),
        Lookup::Ambiguous
    ));
}

#[test]
fn lookup_only_considers_the_same_basename() {
    let coverage = HashMap::from([
        (PathBuf::from("src/foo.rs"), file(1)),
        (PathBuf::from("src/bar.rs"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("/proj/src/foo.rs"), None)),
        Some(1)
    );
    assert!(matches!(
        index.lookup(Path::new("/proj/src/missing.rs"), None),
        Lookup::Missing
    ));
    assert!(matches!(index.lookup(Path::new(""), None), Lookup::Missing));
}

#[test]
fn two_crate_named_suffixes_are_ambiguous() {
    let coverage = HashMap::from([
        (PathBuf::from("/ws/demo/src/lib.rs"), file(1)),
        (PathBuf::from("/other/demo/src/lib.rs"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert!(matches!(
        index.lookup(Path::new("src/lib.rs"), Some("demo")),
        Lookup::Ambiguous
    ));
}

#[test]
fn package_augmented_rank_breaks_equal_suffix_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/packages/pkg_a/src/util.ts"), file(1)),
        (PathBuf::from("/packages/pkg_b/src/util.ts"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("src/util.ts"), Some("pkg_a"))),
        Some(1)
    );
}

#[test]
fn package_component_breaks_root_file_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/packages/pkg_a/util.ts"), file(1)),
        (PathBuf::from("/packages/pkg_b/util.ts"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("util.ts"), Some("pkg_a"))),
        Some(1)
    );
}

#[test]
fn package_component_breaks_lib_layout_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/packages/pkg_a/lib/util.ts"), file(1)),
        (PathBuf::from("/packages/pkg_b/lib/util.ts"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("lib/util.ts"), Some("pkg_a"))),
        Some(1)
    );
}

#[test]
fn scoped_package_last_segment_breaks_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/packages/a/src/util.ts"), file(1)),
        (PathBuf::from("/packages/b/src/util.ts"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("src/util.ts"), Some("@scope/a"))),
        Some(1)
    );
}

#[test]
fn import_path_breaks_remapped_filesystem_tie() {
    let coverage = HashMap::from([
        (PathBuf::from("/mod/pkg_a/foo.go"), file(1)),
        (PathBuf::from("/mod/pkg_b/foo.go"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("foo.go"), Some("example.com/mod/pkg_a"))),
        Some(1)
    );
}

#[test]
fn import_path_matches_unremapped_coverprofile_keys() {
    let coverage = HashMap::from([
        (PathBuf::from("example.com/mod/pkg_a/foo.go"), file(1)),
        (PathBuf::from("example.com/mod/pkg_b/foo.go"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    assert_eq!(
        hit_line(index.lookup(Path::new("foo.go"), Some("example.com/mod/pkg_a"))),
        Some(1)
    );
}

#[test]
fn shared_import_path_last_segment_stays_ambiguous() {
    let coverage = HashMap::from([
        (PathBuf::from("/a/util/foo.go"), file(1)),
        (PathBuf::from("/b/util/foo.go"), file(99)),
    ]);
    let index = PathIndex::from_coverage(&coverage);
    // Last segment alone matches both; longer suffixes match neither.
    assert!(matches!(
        index.lookup(Path::new("foo.go"), Some("example.com/x/util")),
        Lookup::Ambiguous
    ));
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
