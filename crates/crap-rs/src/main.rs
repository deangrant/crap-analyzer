//! Binary entry point for `crap-rs`.

use crap_core::{Error, RunResult, ScanRequest, render, run};
use crap_rs::cli::{self, Action};
use std::io::IsTerminal;
use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::parse() {
        Ok(action) => run_action(action),
        Err(err) => print_usage_err(&err),
    }
}

fn run_action(action: Action) -> ExitCode {
    match action {
        Action::Help => print_ok(&cli::help_text()),
        Action::Version => print_ok(&cli::version_text()),
        Action::Run(args) => run_scan(&args),
    }
}

fn run_scan(args: &cli::Args) -> ExitCode {
    let (lang, request) = args.parts();
    match run(&lang, &request) {
        Ok(result) => finish_run(&request, &result),
        Err(err) => print_core_err(&err),
    }
}

fn finish_run(request: &ScanRequest, result: &RunResult) -> ExitCode {
    for warning in &result.warnings {
        emit_stderr(warning);
    }
    exit_from_render(
        render(request, result, "rust", color_enabled()),
        result.gate_failed,
    )
}

fn exit_from_render(rendered: Result<String, Error>, gate_failed: bool) -> ExitCode {
    match rendered {
        Ok(text) => finish_ok(&text, gate_failed),
        Err(err) => print_core_err(&err),
    }
}

fn finish_ok(text: &str, gate_failed: bool) -> ExitCode {
    emit_stdout(text);
    if gate_failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

fn print_ok(text: &str) -> ExitCode {
    emit_stdout(text);
    ExitCode::SUCCESS
}

fn print_core_err(err: &Error) -> ExitCode {
    emit_stderr(&err.to_string());
    ExitCode::from(2)
}

fn print_usage_err(err: &str) -> ExitCode {
    emit_stderr(err);
    ExitCode::from(2)
}

#[expect(
    clippy::print_stdout,
    reason = "the binary writes the report to stdout on purpose"
)]
fn emit_stdout(text: &str) {
    println!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "warnings and usage errors go to stderr on purpose"
)]
fn emit_stderr(text: &str) {
    eprintln!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_error_exits_two() {
        let err = Error::collect("json report: boom");
        assert_eq!(exit_from_render(Err(err), false), ExitCode::from(2));
    }

    #[test]
    fn render_ok_respects_the_gate() {
        assert_eq!(exit_from_render(Ok("ok".into()), false), ExitCode::SUCCESS);
        assert_eq!(exit_from_render(Ok("ok".into()), true), ExitCode::from(1));
    }

    #[test]
    fn finish_run_emits_warnings() {
        let request = ScanRequest {
            path: std::path::PathBuf::from("."),
            lcov: std::path::PathBuf::from("lcov.info"),
            metric: crap_core::Metric::Cyclomatic,
            threshold: None,
            summary: true,
            fail_above: false,
            missing: crap_core::MissingPolicy::Pessimistic,
            format: crap_core::ReportFormat::Text,
        };
        let result = RunResult {
            warnings: vec!["skipping x".into()],
            ..RunResult::default()
        };
        assert_eq!(finish_run(&request, &result), ExitCode::SUCCESS);
    }
}
