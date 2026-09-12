//! Binary entry point for `crap-ts`.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use crap_core::{finish_run, print_core_err, print_ok, print_usage_err, run};
use crap_ts::cli::{self, Action};
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
        Ok(result) => finish_run(&request, &result, "typescript"),
        Err(err) => print_core_err(&err),
    }
}
