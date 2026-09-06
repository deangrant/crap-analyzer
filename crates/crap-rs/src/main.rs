//! Binary entry point for `crap-rs`.

use crap_core::{Error, RunResult, ScanRequest, render, run};
use crap_rs::cli::{self, Action};
use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::parse() {
        Ok(Action::Help) => print_ok(&cli::help_text()),
        Ok(Action::Version) => print_ok(&cli::version_text()),
        Ok(Action::Run(args)) => {
            let (lang, request) = args.parts();
            match run(&lang, &request) {
                Ok(result) => finish_run(&request, &result),
                Err(err) => print_err(&err),
            }
        }
        Err(err) => print_err(&err),
    }
}

fn finish_run(request: &ScanRequest, result: &RunResult) -> ExitCode {
    for warning in &result.warnings {
        emit_stderr(warning);
    }
    emit_stdout(&render(request, result));
    if result.gate_failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_ok(text: &str) -> ExitCode {
    emit_stdout(text);
    ExitCode::SUCCESS
}

fn print_err(err: &Error) -> ExitCode {
    emit_stderr(&err.to_string());
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
