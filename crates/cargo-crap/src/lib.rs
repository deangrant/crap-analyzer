//! Score Rust functions by combining complexity and test coverage.

pub mod cli;
pub(crate) mod complexity;
pub(crate) mod coverage;
pub mod error;
mod json;
pub(crate) mod merge;
pub(crate) mod report;
pub mod score;
pub(crate) mod walk;
pub(crate) mod workspace;

use crate::cli::Args;
use crate::error::{Error, Result};
use crate::merge::{CrapEntry, LocatedFn};
use crate::score::exceeds_threshold;
use crate::workspace::Package;
use std::path::{Path, PathBuf};

/// Finished analysis ready to print.
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// Functions after join, scored and sorted worst-first.
    pub(crate) entries: Vec<CrapEntry>,
    /// Skipped files that failed to parse.
    pub warnings: Vec<String>,
    /// True when `--fail-above` should trip.
    pub gate_failed: bool,
}

/// Walks sources, joins coverage, and scores every function.
///
/// # Errors
///
/// Returns I/O, metadata, or usage errors. Individual parse failures
/// become warnings and do not abort the run.
pub fn run(args: &Args) -> Result<RunResult> {
    let coverage = coverage::parse_lcov(&args.lcov)?;
    let targets = analysis_targets(args)?;
    let (functions, warnings) = collect_functions(&targets)?;
    let entries = merge::join(&functions, &coverage, args.missing);
    let gate_failed = args.fail_above
        && entries.iter().any(|entry| exceeds_threshold(entry.crap, args.threshold));
    Ok(RunResult {
        entries,
        warnings,
        gate_failed,
    })
}

/// Formats the human report for `result`.
#[must_use]
pub fn render(args: &Args, result: &RunResult) -> String {
    if args.summary {
        report::render_summary(&result.entries, args.threshold, uses_packages(args))
    } else {
        report::render_table(&result.entries, args.threshold, report::color_enabled())
    }
}

const fn uses_packages(args: &Args) -> bool {
    args.workspace || !args.packages.is_empty()
}

struct Target {
    root: PathBuf,
    crate_name: Option<String>,
    skip: Vec<PathBuf>,
}

fn analysis_targets(args: &Args) -> Result<Vec<Target>> {
    if !uses_packages(args) {
        return Ok(vec![Target {
            root: args.path.clone(),
            crate_name: None,
            skip: Vec::new(),
        }]);
    }
    let packages = if args.workspace {
        workspace::all_members(&args.path)?
    } else {
        workspace::selected_members(&args.packages, &args.path)?
    };
    Ok(targets_from_packages(&packages))
}

fn targets_from_packages(packages: &[Package]) -> Vec<Target> {
    packages
        .iter()
        .map(|pkg| Target {
            root: pkg.root.clone(),
            crate_name: Some(pkg.name.clone()),
            skip: workspace::nested_member_roots(&pkg.root, packages),
        })
        .collect()
}

fn collect_functions(targets: &[Target]) -> Result<(Vec<LocatedFn>, Vec<String>)> {
    let mut functions = Vec::new();
    let mut warnings = Vec::new();
    for target in targets {
        let files = walk::rust_files(&target.root, &target.skip)?;
        for file in files {
            take_file(
                &file,
                target.crate_name.as_deref(),
                &mut functions,
                &mut warnings,
            );
        }
    }
    Ok((functions, warnings))
}

fn take_file(
    file: &Path,
    crate_name: Option<&str>,
    functions: &mut Vec<LocatedFn>,
    warnings: &mut Vec<String>,
) {
    match complexity::analyze_file(file) {
        Ok(found) => {
            for function in found {
                functions.push(LocatedFn {
                    function,
                    crate_name: crate_name.map(str::to_owned),
                });
            }
        }
        Err(err) => warnings.push(parse_warning(file, &err)),
    }
}

fn parse_warning(path: &Path, err: &Error) -> String {
    format!("skipping {}: {err}", path.display())
}
