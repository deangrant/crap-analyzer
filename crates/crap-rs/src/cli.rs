//! Command-line flags and help text for `crap-rs`.

use crate::RustLanguage;
use clap::Parser;
use crap_core::{Metric, MissingPolicy, ReportFormat, ScanRequest, parse_threshold};
use std::env;
use std::path::PathBuf;

/// What the process should do after parsing argv.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Print help and exit 0.
    Help,
    /// Print the version and exit 0.
    Version,
    /// Run analysis with these options.
    Run(Args),
}

/// Options for one analysis run.
#[derive(Debug, Clone, PartialEq, Parser)]
#[command(
    name = "crap-rs",
    disable_help_flag = true,
    disable_version_flag = true
)]
pub struct Args {
    /// Coverage file path.
    #[arg(long = "coverage", visible_alias = "lcov", default_value = "lcov.info")]
    pub(crate) coverage: PathBuf,
    /// Walk root. A Cargo workspace at this path is analyzed per member.
    #[arg(long, default_value = ".")]
    pub(crate) path: PathBuf,
    /// Complexity metric.
    #[arg(long, default_value = "cyclomatic")]
    pub(crate) metric: Metric,
    /// Score above which a function is flagged (`strict` = 8, `lenient` = 25).
    #[arg(long, value_parser = parse_threshold)]
    pub(crate) threshold: Option<f64>,
    /// Analyze every workspace member.
    #[arg(long, conflicts_with = "packages")]
    pub(crate) workspace: bool,
    /// Selected package names (`-p`).
    #[arg(short = 'p', long = "package")]
    pub(crate) packages: Vec<String>,
    /// Print counts only.
    #[arg(long)]
    pub(crate) summary: bool,
    /// Exit 1 when any function exceeds `threshold`.
    #[arg(long)]
    pub(crate) fail_above: bool,
    /// Policy for functions with no coverage data.
    #[arg(long, default_value = "pessimistic")]
    pub(crate) missing: MissingPolicy,
    /// Output format.
    #[arg(long, default_value = "text")]
    pub(crate) format: ReportFormat,
    /// Cargo feature flags used for `#[cfg]` evaluation.
    #[command(flatten)]
    pub(crate) features: FeatureArgs,
}

/// Cargo feature flags mirrored from `cargo llvm-cov` / `cargo test`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Parser)]
pub struct FeatureArgs {
    /// Extra Cargo features to treat as enabled.
    #[arg(long, value_delimiter = ',')]
    pub(crate) features: Vec<String>,
    /// Enable every named package feature.
    #[arg(long)]
    pub(crate) all_features: bool,
    /// Do not enable the package `default` feature list.
    #[arg(long)]
    pub(crate) no_default_features: bool,
}

impl Args {
    /// Splits Rust-only flags from the language-agnostic scan request.
    #[must_use]
    pub fn parts(&self) -> (RustLanguage, ScanRequest) {
        (
            RustLanguage {
                workspace: self.workspace,
                packages: self.packages.clone(),
                features: crate::FeatureSelection {
                    features: self.features.features.clone(),
                    all_features: self.features.all_features,
                    no_default_features: self.features.no_default_features,
                },
            },
            ScanRequest {
                path: self.path.clone(),
                coverage: self.coverage.clone(),
                metric: self.metric,
                threshold: self.threshold,
                summary: self.summary,
                fail_above: self.fail_above,
                missing: self.missing,
                format: self.format,
            },
        )
    }
}

/// Parses process arguments.
///
/// # Errors
///
/// Returns a usage message for unknown flags, missing values, or conflicts.
pub fn parse() -> std::result::Result<Action, String> {
    let raw: Vec<String> = env::args().collect();
    parse_args(raw)
}

/// Parses an argv vector (binary name first).
///
/// # Errors
///
/// Returns a usage message for invalid flags.
fn parse_args(raw: Vec<String>) -> std::result::Result<Action, String> {
    if let Some(action) = help_or_version(&raw) {
        return Ok(action);
    }
    let args = Args::try_parse_from(raw).map_err(|err| err.to_string())?;
    reject_summary_json(&args)?;
    Ok(Action::Run(args))
}

fn help_or_version(raw: &[String]) -> Option<Action> {
    if has_flag(raw, "-h", "--help") {
        return Some(Action::Help);
    }
    if has_flag(raw, "-V", "--version") {
        return Some(Action::Version);
    }
    None
}

fn has_flag(raw: &[String], short: &str, long: &str) -> bool {
    raw.iter().skip(1).any(|arg| arg == short || arg == long)
}

fn reject_summary_json(args: &Args) -> std::result::Result<(), String> {
    if args.summary && args.format == ReportFormat::Json {
        return Err("--summary conflicts with --format json".into());
    }
    Ok(())
}

/// Usage and scoring help text.
#[must_use]
pub fn help_text() -> String {
    format!(
        "\
{name} {version}
Score Rust functions by combining complexity and test coverage.

This is a change-risk signal for a function, not a quality score, a
programmer rating, or a management KPI. A high score means the function
is complex, lightly tested by automated tests, or both.

Risk bands (low / acceptable / moderate / high) classify the score.
They never change. The pass/fail gate is --threshold, not the band.

USAGE:
    crap-rs [OPTIONS]

OPTIONS:
    --coverage <file>       Coverage file [default: lcov.info]; must
                            contain at least one DA line-hit. Alias:
                            --lcov. Produce one with:
                            cargo llvm-cov --lcov --output-path lcov.info
    --path <dir>            Walk root [default: .]. A workspace root is
                            analyzed per member; a member package root
                            is that package only (no -p required)
    --metric <name>         cyclomatic (default) or cognitive
    --threshold <n>         Flag scores strictly above this
                            [default: 15; strict={strict}, lenient={lenient}]
    --format <name>         text (default) or json
    --workspace             Analyze every Cargo workspace member
    -p, --package <name>    Analyze only this member (repeatable)
    --summary               Counts and worst offender; text only
                            (conflicts with --format json)
    --fail-above            Exit 1 if any function exceeds --threshold
    --missing <policy>      No LCOV data, empty span, or an unresolved
                            path tie: pessimistic (0%, default),
                            optimistic (100%), or skip. A package name
                            breaks equal basename ties ({{name}}/src|…).
    --features <list>       Extra Cargo features (comma-separated);
                            pass the same set used for cargo llvm-cov
    --all-features          Enable every named package feature
    --no-default-features   Do not enable package default features
    -h, --help              Print help
    -V, --version           Print version

    Scoring formula, coverage table, and metric rules: README.md

EXIT CODES:
    0   Analysis finished; no requested gate tripped
    1   Analysis finished; --fail-above tripped
    2   Usage, input, or analysis error (any source failed to parse)
",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
        strict = crap_core::STRICT,
        lenient = crap_core::LENIENT,
    )
}

/// Crate version string.
#[must_use]
pub fn version_text() -> String {
    format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
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
}
