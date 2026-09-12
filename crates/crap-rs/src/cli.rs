//! Command-line flags and help text for `crap-rs`.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
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
    if let Some(action) = crap_core::help_or_version(&raw) {
        return Ok(help_or_version_action(action));
    }
    run_from_clap(raw)
}

const fn help_or_version_action(action: crap_core::HelpOrVersion) -> Action {
    match action {
        crap_core::HelpOrVersion::Help => Action::Help,
        crap_core::HelpOrVersion::Version => Action::Version,
    }
}

fn run_from_clap(raw: Vec<String>) -> std::result::Result<Action, String> {
    let args = Args::try_parse_from(raw).map_err(|err| err.to_string())?;
    crap_core::reject_summary_json(args.summary, args.format)?;
    Ok(Action::Run(args))
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
                            breaks equal basename ties ({{name}}/src|lib|…).
                            Leftover ties are labeled ambiguous.
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
#[path = "cli_tests.rs"]
mod tests;
