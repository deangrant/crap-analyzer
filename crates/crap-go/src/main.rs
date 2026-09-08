//! Binary entry point for `crap-go`.

use crap_core::{
    FileCoverage, finish_run, print_core_err, print_ok, print_usage_err, run_with_coverage,
};
use crap_go::cli::{self, Action};
use crap_go::coverprofile::{parse_coverprofile, remap_import_paths_all};
use crap_go::modules_for_remap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    match load_coverage(&request.coverage, &request.path)
        .and_then(|coverage| run_with_coverage(&lang, &request, &coverage))
    {
        Ok(result) => finish_run(&request, &result, "go"),
        Err(err) => print_core_err(&err),
    }
}

fn load_coverage(
    coverage_path: &Path,
    analysis_root: &Path,
) -> crap_core::Result<HashMap<PathBuf, FileCoverage>> {
    let coverage = parse_coverprofile(coverage_path)?;
    let modules = modules_for_remap(analysis_root)?;
    if modules.is_empty() {
        return Ok(coverage);
    }
    Ok(remap_import_paths_all(&coverage, &modules))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use crap_go::cli::{self, Action, Args};

    #[test]
    fn version_action_prints_ok() {
        assert_eq!(run_action(Action::Version), ExitCode::SUCCESS);
    }

    #[test]
    fn help_action_prints_ok() {
        assert_eq!(run_action(Action::Help), ExitCode::SUCCESS);
        let _ = cli::help_text();
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
    fn load_coverage_without_module_keeps_raw_paths() {
        let dir = std::env::temp_dir().join(format!(
            "crap-go-nomod-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let created = std::fs::create_dir_all(&dir);
        assert!(created.is_ok(), "{created:?}");
        let cover = dir.join("cover.out");
        let written = std::fs::write(&cover, "mode: set\nmain.go:1.1,1.2 1 1\n");
        assert!(written.is_ok(), "{written:?}");
        let result = load_coverage(&cover, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "{result:?}");
        let map = result.unwrap_or_default();
        assert!(map.contains_key(Path::new("main.go")), "{map:?}");
    }
}
