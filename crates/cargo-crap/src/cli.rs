//! Command-line flags and cargo-style help text.

use crate::error::{Error, Result};
use crate::merge::MissingPolicy;
use crate::score::DEFAULT_THRESHOLD;
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
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    /// LCOV coverage file.
    pub(crate) lcov: PathBuf,
    /// Directory to walk when packages are not selected.
    pub(crate) path: PathBuf,
    /// Score above which a function is flagged.
    pub(crate) threshold: f64,
    /// Analyze every workspace member.
    pub(crate) workspace: bool,
    /// Selected package names (`-p`).
    pub(crate) packages: Vec<String>,
    /// Print counts only.
    pub(crate) summary: bool,
    /// Exit 1 when any function exceeds `threshold`.
    pub(crate) fail_above: bool,
    /// Policy for functions with no coverage data.
    pub(crate) missing: MissingPolicy,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            lcov: PathBuf::new(),
            path: PathBuf::from("."),
            threshold: DEFAULT_THRESHOLD,
            workspace: false,
            packages: Vec::new(),
            summary: false,
            fail_above: false,
            missing: MissingPolicy::Pessimistic,
        }
    }
}

/// Parses process arguments, stripping a leading `crap` cargo subcommand.
///
/// # Errors
///
/// Returns [`Error::Usage`] for unknown flags, missing values, or conflicts.
pub fn parse() -> Result<Action> {
    let raw: Vec<String> = env::args().collect();
    parse_args(raw)
}

/// Parses an argv vector (binary name first).
///
/// # Errors
///
/// Returns [`Error::Usage`] for invalid flags.
fn parse_args(mut raw: Vec<String>) -> Result<Action> {
    if raw.get(1).is_some_and(|a| a == "crap") {
        raw.remove(1);
    }
    let mut args = Args::default();
    let mut rest = raw.into_iter().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "--summary" => args.summary = true,
            "--fail-above" => args.fail_above = true,
            "--workspace" => args.workspace = true,
            "-p" | "--package" => {
                args.packages.push(require_value(&arg, rest.next())?);
            }
            "--lcov" => args.lcov = PathBuf::from(require_value(&arg, rest.next())?),
            "--path" => args.path = PathBuf::from(require_value(&arg, rest.next())?),
            "--threshold" => args.threshold = parse_threshold(&require_value(&arg, rest.next())?)?,
            "--missing" => args.missing = parse_missing(&require_value(&arg, rest.next())?)?,
            other if other.starts_with('-') => {
                return Err(Error::usage(format!("unknown flag `{other}`")));
            }
            other => {
                return Err(Error::usage(format!("unexpected argument `{other}`")));
            }
        }
    }
    validate(&args)?;
    Ok(Action::Run(args))
}

fn require_value(flag: &str, value: Option<String>) -> Result<String> {
    value.ok_or_else(|| Error::usage(format!("{flag} requires a value")))
}

fn parse_threshold(text: &str) -> Result<f64> {
    let value: f64 = text
        .parse()
        .map_err(|_| Error::usage(format!("invalid --threshold `{text}`")))?;
    if !value.is_finite() || value < 0.0 {
        return Err(Error::usage("--threshold must be a non-negative number"));
    }
    Ok(value)
}

fn parse_missing(text: &str) -> Result<MissingPolicy> {
    match text {
        "pessimistic" => Ok(MissingPolicy::Pessimistic),
        "optimistic" => Ok(MissingPolicy::Optimistic),
        "skip" => Ok(MissingPolicy::Skip),
        other => Err(Error::usage(format!(
            "invalid --missing `{other}` (pessimistic|optimistic|skip)"
        ))),
    }
}

fn validate(args: &Args) -> Result<()> {
    if args.lcov.as_os_str().is_empty() {
        return Err(Error::usage("missing required --lcov <file>"));
    }
    if args.workspace && !args.packages.is_empty() {
        return Err(Error::usage("--workspace conflicts with --package / -p"));
    }
    Ok(())
}

/// Cargo-style help text.
#[must_use]
pub fn help_text() -> String {
    format!(
        "\
{name} {version}
Score Rust functions by combining cyclomatic complexity and test coverage.

This is a change-risk signal for a function, not a quality score, a
programmer rating, or a management KPI. A high score means the function
is complex, lightly tested by automated tests, or both.

USAGE:
    cargo crap [OPTIONS]
    cargo-crap [OPTIONS]

OPTIONS:
    --lcov <file>           LCOV file (required). Produce one with:
                            cargo llvm-cov --lcov --output-path lcov.info
    --path <dir>            Root to walk for .rs files [default: .]
                            Ignored when --workspace or -p is set
    --threshold <n>         Flag scores strictly above this [default: 30]
                            Lower values are valid for stricter gates
    --workspace             Analyze every Cargo workspace member
    -p, --package <name>    Analyze only this member (repeatable)
    --summary               Counts and worst offender; no table
    --fail-above            Exit 1 if any function exceeds --threshold
    --missing <policy>      No coverage data: pessimistic (0%, default),
                            optimistic (100%), or skip
    -h, --help              Print help
    -V, --version           Print version

SCORE:
    CRAP(m) = CC^2 * (1 - cov/100)^3 + CC

    100% coverage => score equals complexity (never zero).
    0% coverage   => CC^2 + CC.
    CC 31 or more cannot fall to 30 or below at any coverage.

    Coverage needed to stay at or under 30 (usual line):
      CC 0-5    0%     CC 16-20   ~71%
      CC 6-10   ~42%   CC 21-25   ~80%
      CC 11-15  ~57%   CC 26-30   100%
      CC 31+    refactor; coverage cannot help

    A low score is not a reason to skip tests on simple functions.
    If a function is flagged: add automated tests when coverage is low;
    extract or simplify when complexity stays high even when covered.

    Coverage here is instrumented-line hits from the LCOV file. It does
    not prove assertions are meaningful. Coupling and cohesion are out
    of scope. Some complex functions are legitimate.

EXIT CODES:
    0   Analysis finished; no requested gate tripped
    1   Analysis finished; --fail-above tripped
    2   Usage, input, or analysis error
",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
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
        std::iter::once("cargo-crap")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn strips_cargo_subcommand_and_parses_flags() {
        let action = parse_args(argv(&["crap", "--lcov", "lcov.info", "--summary"]));
        assert!(matches!(
            action,
            Ok(Action::Run(ref args))
                if args.lcov == Path::new("lcov.info") && args.summary && !args.fail_above
        ));
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert!(matches!(parse_args(argv(&["--help"])), Ok(Action::Help)));
        assert!(matches!(
            parse_args(argv(&["crap", "-V"])),
            Ok(Action::Version)
        ));
    }

    #[test]
    fn missing_lcov_is_usage() {
        assert!(parse_args(argv(&[])).is_err());
    }

    #[test]
    fn workspace_conflicts_with_package() {
        let err = parse_args(argv(&["--lcov", "x.info", "--workspace", "-p", "core"]));
        assert!(err.is_err());
    }

    #[test]
    fn unknown_flag_is_usage() {
        assert!(parse_args(argv(&["--lcov", "x", "--nope"])).is_err());
    }

    #[test]
    fn parses_missing_policy() {
        let action = parse_args(argv(&["--lcov", "x", "--missing", "skip"]));
        assert!(matches!(
            action,
            Ok(Action::Run(ref args)) if args.missing == MissingPolicy::Skip
        ));
    }
}
