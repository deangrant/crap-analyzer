//! Binary entry point for the `cargo crap` subcommand.

use cargo_crap::cli::{self, Action};
use cargo_crap::error::Error;
use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::parse() {
        Ok(Action::Help) => print_ok(&cli::help_text()),
        Ok(Action::Version) => print_ok(&cli::version_text()),
        Ok(Action::Run(args)) => match cargo_crap::run(&args) {
            Ok(result) => finish_run(&args, &result),
            Err(err) => print_err(&err),
        },
        Err(err) => print_err(&err),
    }
}

fn finish_run(args: &cli::Args, result: &cargo_crap::RunResult) -> ExitCode {
    for warning in &result.warnings {
        emit_stderr(warning);
    }
    emit_stdout(&cargo_crap::render(args, result));
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
