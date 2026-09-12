// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::*;
use std::cmp::Ordering;
use std::path::Path;

fn argv(args: &[&str]) -> Vec<String> {
    std::iter::once("crap-rs")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect()
}

fn threshold_eq(args: &Args, expected: f64) -> bool {
    args.parts().1.effective_threshold().total_cmp(&expected) == Ordering::Equal
}

#[test]
fn parses_flags() {
    let action = parse_args(argv(&["--coverage", "lcov.info", "--summary"]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args))
            if args.coverage == Path::new("lcov.info") && args.summary && !args.fail_above
    ));
}

#[test]
fn help_and_version_short_circuit() {
    assert!(matches!(parse_args(argv(&["--help"])), Ok(Action::Help)));
    assert!(matches!(parse_args(argv(&["-V"])), Ok(Action::Version)));
}

#[test]
fn default_coverage_is_lcov_info() {
    let action = parse_args(argv(&[]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args)) if args.coverage == Path::new("lcov.info")
    ));
}

#[test]
fn lcov_alias_still_works() {
    let action = parse_args(argv(&["--lcov", "alt.info"]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args)) if args.coverage == Path::new("alt.info")
    ));
}

#[test]
fn workspace_conflicts_with_package() {
    let err = parse_args(argv(&["--coverage", "x.info", "--workspace", "-p", "core"]));
    assert!(err.is_err());
}

#[test]
fn summary_conflicts_with_json() {
    let err = parse_args(argv(&[
        "--coverage",
        "x.info",
        "--summary",
        "--format",
        "json",
    ]));
    assert!(err.is_err());
}

#[test]
fn unknown_flag_is_usage() {
    assert!(parse_args(argv(&["--coverage", "x", "--nope"])).is_err());
}

#[test]
fn parses_missing_policy() {
    let action = parse_args(argv(&["--lcov", "x", "--missing", "skip"]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args)) if args.missing == MissingPolicy::Skip
    ));
}

#[test]
fn default_metric_is_cyclomatic_with_threshold_fifteen() {
    let action = parse_args(argv(&["--coverage", "x"]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args))
            if args.metric == Metric::Cyclomatic
                && args.format == ReportFormat::Text
                && threshold_eq(args, 15.0)
    ));
}

#[test]
fn cognitive_default_threshold_is_fifteen() {
    let action = parse_args(argv(&["--lcov", "x", "--metric", "cognitive"]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args))
            if args.metric == Metric::Cognitive && threshold_eq(args, 15.0)
    ));
}

fn parsed_threshold(flag: &str) -> Option<f64> {
    match parse_args(argv(&["--coverage", "x", "--threshold", flag])) {
        Ok(Action::Run(args)) => Some(args.parts().1.effective_threshold()),
        _ => None,
    }
}

fn run_args(action: Result<Action, String>) -> Option<Args> {
    match action {
        Ok(Action::Run(args)) => Some(args),
        Ok(Action::Help | Action::Version) | Err(_) => None,
    }
}

fn fallback_args() -> Args {
    Args::parse_from(["crap-rs", "--lcov", "x"])
}

#[test]
fn threshold_presets() {
    let presets = [("strict", 8.0), ("lenient", 25.0)];
    for (flag, expected) in presets {
        let got = parsed_threshold(flag);
        assert!(got.is_some_and(|value| value.total_cmp(&expected) == Ordering::Equal));
    }
    assert!(parsed_threshold("abc").is_none());
}

#[test]
fn parses_format() {
    let action = parse_args(argv(&["--coverage", "x", "--format", "json"]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args)) if args.format == ReportFormat::Json
    ));
}

#[test]
fn invalid_format_is_usage() {
    assert!(parse_args(argv(&["--lcov", "x", "--format", "nope"])).is_err());
}

#[test]
fn explicit_threshold_wins() {
    let action = parse_args(argv(&[
        "--coverage",
        "x",
        "--metric",
        "cognitive",
        "--threshold",
        "8",
    ]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args)) if threshold_eq(args, 8.0)
    ));
}

#[test]
fn version_text_includes_crate_name_and_version() {
    let text = version_text();
    assert!(text.starts_with("crap-rs "));
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn invalid_threshold_is_usage() {
    assert!(parse_args(argv(&["--coverage", "x", "--threshold", "abc"])).is_err());
    assert!(parse_args(argv(&["--lcov", "x", "--threshold", "-1"])).is_err());
    assert!(parse_args(argv(&["--coverage", "x", "--threshold", "inf"])).is_err());
}

#[test]
fn parses_feature_flags() {
    let action = parse_args(argv(&[
        "--lcov",
        "x",
        "--features",
        "serde,std",
        "--no-default-features",
    ]));
    assert!(matches!(
        action,
        Ok(Action::Run(ref args))
            if args.features.features == ["serde".to_owned(), "std".to_owned()]
                && args.features.no_default_features
                && !args.features.all_features
    ));
}

#[test]
fn leftover_crap_token_is_usage() {
    assert!(parse_args(argv(&["crap", "--coverage", "x"])).is_err());
}

#[test]
fn parts_split_rust_flags_from_the_request() {
    let action = parse_args(argv(&[
        "--coverage",
        "x.info",
        "--workspace",
        "--summary",
        "--fail-above",
    ]));
    let args = run_args(action).unwrap_or_else(fallback_args);
    let (lang, request) = args.parts();
    assert!(
        lang.workspace
            && !lang.features.all_features
            && request.summary
            && request.fail_above
            && request.coverage == Path::new("x.info")
    );
    let skipped = run_args(Ok(Action::Help)).unwrap_or_else(fallback_args);
    assert!(!skipped.workspace);
    assert!(run_args(Ok(Action::Version)).is_none());
    assert!(run_args(Err("nope".into())).is_none());
}
