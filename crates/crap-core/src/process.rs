//! Shared binary process helpers for the language frontends.
//!
//! Exit codes, stdout/stderr emission, and argv short-circuits live here so
//! `crap-rs` / `crap-go` / `crap-ts` do not drift. Clap `Args` and help text
//! stay in each frontend.

use crate::language::ReportFormat;
use crate::merge::ambiguous_join_warning;
use crate::report::write_report;
use crate::{Error, RunResult, ScanRequest};
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

/// Short-circuit argv result before clap parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpOrVersion {
    /// `-h` / `--help`.
    Help,
    /// `-V` / `--version`.
    Version,
}

/// Returns help/version when those flags appear after the binary name.
#[must_use]
pub fn help_or_version(raw: &[String]) -> Option<HelpOrVersion> {
    if has_flag(raw, "-h", "--help") {
        return Some(HelpOrVersion::Help);
    }
    if has_flag(raw, "-V", "--version") {
        return Some(HelpOrVersion::Version);
    }
    None
}

fn has_flag(raw: &[String], short: &str, long: &str) -> bool {
    raw.iter().skip(1).any(|arg| arg == short || arg == long)
}

/// Rejects `--summary` combined with `--format json`.
///
/// # Errors
///
/// Returns a usage message when the flags conflict.
pub fn reject_summary_json(summary: bool, format: ReportFormat) -> std::result::Result<(), String> {
    if summary && format == ReportFormat::Json {
        return Err("--summary conflicts with --format json".into());
    }
    Ok(())
}

/// Renders the report, emits ambiguous-join warnings, and maps the gate to an exit.
#[must_use]
pub fn finish_run(request: &ScanRequest, result: &RunResult, language: &str) -> ExitCode {
    emit_ambiguous_warning(result);
    let written = write_finished_report(request, result, language, color_enabled());
    exit_from_write(written, result.gate_failed)
}

fn write_finished_report(
    request: &ScanRequest,
    result: &RunResult,
    language: &str,
    color: bool,
) -> Result<(), Error> {
    let mut stdout = io::stdout().lock();
    write_report_and_flush(&mut stdout, request, result, language, color)
}

fn write_report_and_flush(
    w: &mut impl Write,
    request: &ScanRequest,
    result: &RunResult,
    language: &str,
    color: bool,
) -> Result<(), Error> {
    write_report(w, request, result, language, color)?;
    writeln!(w).map_err(|err| report_write_err(&err))?;
    w.flush().map_err(|err| report_write_err(&err))
}

fn report_write_err(err: &io::Error) -> Error {
    Error::report(format!("report write: {err}"))
}

fn exit_from_write(written: Result<(), Error>, gate_failed: bool) -> ExitCode {
    match written {
        Ok(()) => {
            if gate_failed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => print_core_err(&err),
    }
}

fn emit_ambiguous_warning(result: &RunResult) {
    let warning = ambiguous_join_warning(&result.entries);
    if !warning.is_empty() {
        emit_stderr(&warning);
    }
}

fn color_enabled() -> bool {
    color_from_env(
        std::env::var_os("NO_COLOR").is_none(),
        std::io::stdout().is_terminal(),
    )
}

const fn color_from_env(allow_color: bool, is_tty: bool) -> bool {
    allow_color && is_tty
}

/// Writes `text` to stdout and returns success.
#[must_use]
pub fn print_ok(text: &str) -> ExitCode {
    emit_stdout(text);
    ExitCode::SUCCESS
}

/// Writes an analysis error to stderr and returns exit 2.
#[must_use]
pub fn print_core_err(err: &Error) -> ExitCode {
    emit_stderr(&err.to_string());
    ExitCode::from(2)
}

/// Writes a usage message to stderr and returns exit 2.
#[must_use]
pub fn print_usage_err(err: &str) -> ExitCode {
    emit_stderr(err);
    ExitCode::from(2)
}

#[expect(
    clippy::print_stdout,
    reason = "frontend binaries write the report to stdout on purpose"
)]
fn emit_stdout(text: &str) {
    println!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "usage and analysis errors go to stderr on purpose"
)]
fn emit_stderr(text: &str) {
    eprintln!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoverageJoin, CrapEntry, Metric, MissingPolicy, ReportFormat, RunResult};
    use std::path::PathBuf;

    #[test]
    fn help_or_version_detects_short_and_long() {
        assert_eq!(
            help_or_version(&["bin".into(), "-h".into()]),
            Some(HelpOrVersion::Help)
        );
        assert_eq!(
            help_or_version(&["bin".into(), "--version".into()]),
            Some(HelpOrVersion::Version)
        );
        assert_eq!(help_or_version(&["bin".into(), "--summary".into()]), None);
    }

    #[test]
    fn reject_summary_json_conflicts() {
        assert!(reject_summary_json(true, ReportFormat::Json).is_err());
        assert!(reject_summary_json(true, ReportFormat::Text).is_ok());
        assert!(reject_summary_json(false, ReportFormat::Json).is_ok());
    }

    #[test]
    fn render_error_exits_two() {
        let err = Error::report("json report: boom");
        assert_eq!(exit_from_write(Err(err), false), ExitCode::from(2));
    }

    #[test]
    fn render_ok_respects_the_gate() {
        assert_eq!(exit_from_write(Ok(()), false), ExitCode::SUCCESS);
        assert_eq!(exit_from_write(Ok(()), true), ExitCode::from(1));
    }

    #[test]
    fn usage_error_exits_two() {
        assert_eq!(print_usage_err("bad flag"), ExitCode::from(2));
    }

    #[test]
    fn print_ok_writes_and_succeeds() {
        assert_eq!(print_ok("ready"), ExitCode::SUCCESS);
    }

    #[test]
    fn write_report_and_flush_maps_io_errors() {
        struct FailAfter {
            ok_writes: usize,
            seen: usize,
            flush_ok: bool,
        }
        impl Write for FailAfter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if self.seen >= self.ok_writes {
                    return Err(io::Error::other("write boom"));
                }
                self.seen += 1;
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                if self.flush_ok {
                    Ok(())
                } else {
                    Err(io::Error::other("flush boom"))
                }
            }
        }
        let request = summary_request("lcov.info");
        let result = RunResult::default();
        let write_err = write_report_and_flush(
            &mut FailAfter {
                ok_writes: 0,
                seen: 0,
                flush_ok: true,
            },
            &request,
            &result,
            "rust",
            false,
        );
        assert!(write_err.is_err(), "{write_err:?}");
        let newline_err = write_report_and_flush(
            &mut FailAfter {
                ok_writes: 1,
                seen: 0,
                flush_ok: true,
            },
            &request,
            &result,
            "rust",
            false,
        );
        assert!(newline_err.is_err(), "{newline_err:?}");
        let flush_err = write_report_and_flush(
            &mut FailAfter {
                ok_writes: 32,
                seen: 0,
                flush_ok: false,
            },
            &request,
            &result,
            "rust",
            false,
        );
        assert!(flush_err.is_err(), "{flush_err:?}");
        let flushed_ok = write_report_and_flush(
            &mut FailAfter {
                ok_writes: 32,
                seen: 0,
                flush_ok: true,
            },
            &request,
            &result,
            "rust",
            false,
        );
        assert!(flushed_ok.is_ok(), "{flushed_ok:?}");
        assert!(report_write_err(&io::Error::other("x")).to_string().contains("report write"));
    }

    #[test]
    fn color_from_env_requires_tty_and_no_color_unset() {
        assert!(color_from_env(true, true));
        assert!(!color_from_env(true, false));
        assert!(!color_from_env(false, true));
        assert!(!color_from_env(false, false));
    }

    fn summary_request(coverage: &str) -> ScanRequest {
        ScanRequest {
            path: PathBuf::from("."),
            coverage: PathBuf::from(coverage),
            metric: Metric::Cyclomatic,
            threshold: None,
            summary: true,
            fail_above: false,
            missing: MissingPolicy::Pessimistic,
            format: ReportFormat::Text,
        }
    }

    #[test]
    fn finish_run_renders_summary() {
        let request = summary_request("lcov.info");
        let result = RunResult::default();
        assert_eq!(finish_run(&request, &result, "rust"), ExitCode::SUCCESS);
    }

    #[test]
    fn finish_run_renders_json() {
        let mut request = summary_request("lcov.info");
        request.summary = false;
        request.format = ReportFormat::Json;
        let result = RunResult::default();
        assert_eq!(finish_run(&request, &result, "rust"), ExitCode::SUCCESS);
    }

    #[test]
    fn finish_run_emits_ambiguous_path_warning() {
        let request = summary_request("lcov.info");
        let result = RunResult {
            entries: vec![CrapEntry {
                file: PathBuf::from("src/lib.rs"),
                function: "tied".into(),
                start_line: 1,
                end_line: 1,
                complexity: 1,
                coverage: 0.0,
                coverage_join: CoverageJoin::Ambiguous,
                crap: 2.0,
                crate_name: None,
            }],
            gate_failed: false,
        };
        assert_eq!(finish_run(&request, &result, "rust"), ExitCode::SUCCESS);
    }
}
