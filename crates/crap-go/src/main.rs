//! Binary entry point for `crap-go`.

use crap_core::{Error, RunResult, ScanRequest, render, run_with_coverage};
use crap_go::cli::{self, Action};
use crap_go::coverprofile::parse_coverprofile;
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
    match parse_coverprofile(&request.coverage)
        .and_then(|coverage| run_with_coverage(&lang, &request, &coverage))
    {
        Ok(result) => finish_run(&request, &result),
        Err(err) => print_core_err(&err),
    }
}

fn finish_run(request: &ScanRequest, result: &RunResult) -> ExitCode {
    for warning in &result.warnings {
        emit_stderr(warning);
    }
    exit_from_render(
        render(request, result, "go", color_enabled()),
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
    use clap::Parser;
    use crap_go::cli::{self, Action, Args};
    use std::path::PathBuf;

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
    fn usage_error_exits_two() {
        assert_eq!(print_usage_err("bad flag"), ExitCode::from(2));
    }

    #[test]
    fn version_action_prints_ok() {
        assert_eq!(run_action(Action::Version), ExitCode::SUCCESS);
    }

    #[test]
    fn finish_run_emits_warnings() {
        let request = ScanRequest {
            path: std::path::PathBuf::from("."),
            coverage: std::path::PathBuf::from("cover.out"),
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

    #[test]
    fn run_scan_missing_coverage_exits_two() {
        let args = Args::parse_from(["crap-go", "--coverage", "/no/such/cover.out", "--path", "."]);
        assert_eq!(run_scan(&args), ExitCode::from(2));
    }

    #[test]
    fn run_scan_sample_fixture_exits_ok() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
        let coverage = root.join("cover.out");
        let args = Args::parse_from([
            "crap-go",
            "--coverage",
            coverage.to_str().unwrap_or("cover.out"),
            "--path",
            root.to_str().unwrap_or("."),
            "--summary",
        ]);
        let code = run_scan(&args);
        assert!(
            code == ExitCode::SUCCESS || code == ExitCode::from(1),
            "{code:?}"
        );
    }

    #[test]
    fn run_scan_sample_cognitive_and_build_tags() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
        let coverage = root.join("cover.out");
        let args = Args::parse_from([
            "crap-go",
            "--coverage",
            coverage.to_str().unwrap_or("cover.out"),
            "--path",
            root.to_str().unwrap_or("."),
            "--metric",
            "cognitive",
            "--tags",
            "fancy",
            "--summary",
        ]);
        let code = run_scan(&args);
        assert!(
            code == ExitCode::SUCCESS || code == ExitCode::from(1),
            "{code:?}"
        );
    }

    #[test]
    fn help_action_prints_ok() {
        assert_eq!(run_action(Action::Help), ExitCode::SUCCESS);
        let _ = cli::help_text();
    }
}
