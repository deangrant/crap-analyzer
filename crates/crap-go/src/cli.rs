//! Command-line flags and help text for `crap-go`.

use crate::GoLanguage;
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
    name = "crap-go",
    disable_help_flag = true,
    disable_version_flag = true
)]
pub struct Args {
    /// Coverage file path (Go coverprofile).
    #[arg(long = "coverage", default_value = "cover.out")]
    pub(crate) coverage: PathBuf,
    /// Walk root. A module root (`go.mod`) is analyzed per package.
    #[arg(long, default_value = ".")]
    pub(crate) path: PathBuf,
    /// Complexity metric.
    #[arg(long, default_value = "cyclomatic")]
    pub(crate) metric: Metric,
    /// Score above which a function is flagged (`strict` = 8, `lenient` = 25).
    #[arg(long, value_parser = parse_threshold)]
    pub(crate) threshold: Option<f64>,
    /// Analyze every package under the module.
    #[arg(long, conflicts_with = "packages")]
    pub(crate) workspace: bool,
    /// Selected package import paths (`-p`).
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
    /// Build tags treated as enabled (comma-separated).
    #[arg(long, value_delimiter = ',')]
    pub(crate) tags: Vec<String>,
}

impl Args {
    /// Splits Go-only flags from the language-agnostic scan request.
    #[must_use]
    pub fn parts(&self) -> (GoLanguage, ScanRequest) {
        (
            GoLanguage {
                workspace: self.workspace,
                packages: self.packages.clone(),
                tags: self.tags.clone(),
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
fn parse_args(raw: Vec<String>) -> std::result::Result<Action, String> {
    if let Some(action) = help_or_version(&raw) {
        return Ok(action);
    }
    let args = Args::try_parse_from(raw).map_err(|err| err.to_string())?;
    reject_summary_json(&args)?;
    Ok(Action::Run(args))
}

fn help_or_version(raw: &[String]) -> Option<Action> {
    // Keep paired with crap-rs::cli::help_or_version.
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
    // Keep paired with crap-rs::cli::reject_summary_json.
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
Score Go functions by combining complexity and test coverage.

This is a change-risk signal for a function, not a quality score, a
programmer rating, or a management KPI. A high score means the function
is complex, lightly tested by automated tests, or both.

Risk bands (low / acceptable / moderate / high) classify the score.
They never change. The pass/fail gate is --threshold, not the band.

USAGE:
    crap-go [OPTIONS]

OPTIONS:
    --coverage <file>       Go coverprofile [default: cover.out]; must
                            contain a mode line and at least one data
                            line. Produce one with:
                            go test -coverprofile=cover.out ./...
    --path <dir>            Walk root [default: .]. A module root
                            (go.mod) is analyzed per package
    --metric <name>         cyclomatic (default) or cognitive
    --threshold <n>         Flag scores strictly above this
                            [default: 15; strict={strict}, lenient={lenient}]
    --format <name>         text (default) or json
    --workspace             Analyze every package under the module
    -p, --package <path>    Analyze only this import path (repeatable)
    --summary               Counts and worst offender; text only
                            (conflicts with --format json)
    --fail-above            Exit 1 if any function exceeds --threshold
    --missing <policy>      No coverage data, empty span, or an
                            unresolved path tie: pessimistic (0%,
                            default), optimistic (100%), or skip.
                            An import path breaks equal basename ties.
                            Leftover ties are labeled ambiguous.
    --tags <list>           Build tags treated as enabled
                            (comma-separated)
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
    use std::path::Path;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("crap-go")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn help_short_circuits() {
        assert!(matches!(parse_args(argv(&["--help"])), Ok(Action::Help)));
    }

    #[test]
    fn version_short_circuits() {
        assert!(matches!(parse_args(argv(&["-V"])), Ok(Action::Version)));
        assert!(matches!(
            parse_args(argv(&["--version"])),
            Ok(Action::Version)
        ));
    }

    #[test]
    fn default_coverage_is_cover_out() {
        let action = parse_args(argv(&[]));
        assert!(matches!(
            action,
            Ok(Action::Run(ref args)) if args.coverage == Path::new("cover.out")
        ));
    }

    #[test]
    fn parses_tags() {
        let action = parse_args(argv(&["--tags", "fancy,integration"]));
        assert!(matches!(
            action,
            Ok(Action::Run(ref args))
                if args.tags == ["fancy".to_owned(), "integration".to_owned()]
        ));
    }

    fn threshold_of(flag: &str) -> Option<f64> {
        let Ok(Action::Run(args)) = parse_args(argv(&["--threshold", flag])) else {
            return None;
        };
        args.threshold
    }

    #[test]
    fn threshold_presets_and_numbers() {
        assert_eq!(threshold_of("strict"), Some(8.0));
        assert_eq!(threshold_of("lenient"), Some(25.0));
        assert_eq!(threshold_of("12.5"), Some(12.5));
        assert_eq!(threshold_of("abc"), None);
    }

    #[test]
    fn invalid_threshold_is_usage() {
        assert!(parse_args(argv(&["--threshold", "abc"])).is_err());
        assert!(parse_args(argv(&["--threshold", "-1"])).is_err());
        assert!(parse_args(argv(&["--threshold", "inf"])).is_err());
    }

    #[test]
    fn version_text_includes_crate_name() {
        let text = version_text();
        assert!(text.starts_with("crap-go "));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn parts_split_go_flags_from_the_request() {
        let action = parse_args(argv(&[
            "--coverage",
            "c.out",
            "--workspace",
            "--summary",
            "--fail-above",
            "--metric",
            "cognitive",
        ]));
        assert!(
            matches!(
                action,
                Ok(Action::Run(ref args))
                    if {
                        let (lang, request) = args.parts();
                        lang.workspace
                            && request.summary
                            && request.fail_above
                            && request.coverage == Path::new("c.out")
                            && request.metric == Metric::Cognitive
                    }
            ),
            "{action:?}"
        );
    }

    #[test]
    fn workspace_conflicts_with_package() {
        assert!(parse_args(argv(&["--workspace", "-p", "pkg"])).is_err());
    }

    #[test]
    fn summary_conflicts_with_json() {
        assert!(parse_args(argv(&["--summary", "--format", "json"])).is_err());
    }
}
