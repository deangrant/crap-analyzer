//! Go frontend: discover module packages and collect function complexity.

pub(crate) mod build_tag;
pub mod cli;
pub(crate) mod complexity;
pub mod coverprofile;
pub(crate) mod module_resolve;
pub(crate) mod walk;

#[doc(inline)]
pub use module_resolve::{enclosing_module, modules_for_remap};

use crap_core::{Error, Language, LocatedFn, Metric, Result, ScanRequest, Target};
use module_resolve::Package;
use std::path::Path;

/// Go implementation of [`Language`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoLanguage {
    /// Analyze every package under the module.
    pub workspace: bool,
    /// Selected package import paths (`-p`).
    pub packages: Vec<String>,
    /// Build tags treated as enabled for `//go:build`.
    pub tags: Vec<String>,
}

impl Language for GoLanguage {
    fn resolve_targets(&self, request: &ScanRequest) -> Result<Vec<Target>> {
        if self.uses_packages() {
            return self.targets_from_selection(request);
        }
        if let Some(targets) = self.targets_from_module_root(&request.path)? {
            return Ok(targets);
        }
        Ok(vec![self.path_target(request)])
    }

    fn collect_functions(&self, targets: &[Target], metric: Metric) -> Result<Vec<LocatedFn>> {
        collect_functions(targets, metric)
    }
}

impl GoLanguage {
    const fn uses_packages(&self) -> bool {
        self.workspace || !self.packages.is_empty()
    }

    fn targets_from_packages(&self, packages: &[Package]) -> Vec<Target> {
        packages
            .iter()
            .map(|pkg| Target {
                root: pkg.root.clone(),
                // Go import path (language-agnostic package key).
                crate_name: Some(pkg.name.clone()),
                join_key: None,
                skip: module_resolve::nested_module_roots(&pkg.root, packages),
                // Go build tags treated as enabled.
                enabled_features: self.tags.clone(),
            })
            .collect()
    }

    fn targets_from_selection(&self, request: &ScanRequest) -> Result<Vec<Target>> {
        let packages = if self.workspace {
            module_resolve::all_packages(&request.path)?
        } else {
            module_resolve::selected_packages(&self.packages, &request.path)?
        };
        Ok(self.targets_from_packages(&packages))
    }

    fn targets_from_module_root(&self, path: &Path) -> Result<Option<Vec<Target>>> {
        if !path.join("go.mod").is_file() {
            return Ok(None);
        }
        let packages = module_resolve::all_packages(path)?;
        Ok(Some(self.targets_from_packages(&packages)))
    }

    fn path_target(&self, request: &ScanRequest) -> Target {
        Target {
            root: request.path.clone(),
            crate_name: None,
            join_key: None,
            skip: Vec::new(),
            // Go build tags treated as enabled.
            enabled_features: self.tags.clone(),
        }
    }
}

fn collect_functions(targets: &[Target], metric: Metric) -> Result<Vec<LocatedFn>> {
    let mut functions = Vec::new();
    let mut details = Vec::new();
    let (succeeded, failed) = collect_targets(targets, metric, &mut functions, &mut details)?;
    if failed > 0 {
        return Err(Error::collect(collect_failure(
            "Go", failed, succeeded, &details,
        )));
    }
    Ok(functions)
}

fn collect_failure(lang: &str, failed: usize, succeeded: usize, details: &[String]) -> String {
    format!(
        "failed to parse {failed} of {} {lang} file(s)\n{}",
        failed + succeeded,
        details.join("\n")
    )
}

fn collect_targets(
    targets: &[Target],
    metric: Metric,
    functions: &mut Vec<LocatedFn>,
    details: &mut Vec<String>,
) -> Result<(usize, usize)> {
    let mut succeeded = 0_usize;
    let mut failed = 0_usize;
    for target in targets {
        collect_target(
            target,
            metric,
            functions,
            details,
            &mut succeeded,
            &mut failed,
        )?;
    }
    Ok((succeeded, failed))
}

fn collect_target(
    target: &Target,
    metric: Metric,
    functions: &mut Vec<LocatedFn>,
    details: &mut Vec<String>,
    succeeded: &mut usize,
    failed: &mut usize,
) -> Result<()> {
    let files = walk::go_files(&target.root, &target.skip)?;
    for file in files {
        apply_file_outcome(
            take_file(
                &file,
                target.crate_name.as_deref(),
                metric,
                &target.enabled_features,
                functions,
                details,
            ),
            succeeded,
            failed,
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum FileOutcome {
    Ok,
    Skipped,
    Failed,
}

const fn apply_file_outcome(outcome: FileOutcome, succeeded: &mut usize, failed: &mut usize) {
    match outcome {
        FileOutcome::Ok => *succeeded += 1,
        FileOutcome::Skipped => {}
        FileOutcome::Failed => *failed += 1,
    }
}

fn take_file(
    file: &Path,
    crate_name: Option<&str>,
    metric: Metric,
    tags: &[String],
    functions: &mut Vec<LocatedFn>,
    details: &mut Vec<String>,
) -> FileOutcome {
    let Some(source) = read_source(file, details) else {
        return FileOutcome::Failed;
    };
    if build_tag::skip_file(&source, tags) {
        return FileOutcome::Skipped;
    }
    analyze_into(file, &source, crate_name, metric, functions, details)
}

fn read_source(file: &Path, details: &mut Vec<String>) -> Option<String> {
    match std::fs::read_to_string(file) {
        Ok(text) => Some(text),
        Err(err) => {
            details.push(format!("skipping {}: {err}", file.display()));
            None
        }
    }
}

fn analyze_into(
    file: &Path,
    source: &str,
    crate_name: Option<&str>,
    metric: Metric,
    functions: &mut Vec<LocatedFn>,
    details: &mut Vec<String>,
) -> FileOutcome {
    match complexity::analyze_source(file, source, metric) {
        Ok(found) => {
            push_located(found, crate_name, None, functions);
            FileOutcome::Ok
        }
        Err(err) => {
            details.push(format!("skipping {}: {err}", file.display()));
            FileOutcome::Failed
        }
    }
}

fn push_located(
    found: Vec<crap_core::FunctionComplexity>,
    crate_name: Option<&str>,
    join_key: Option<&str>,
    functions: &mut Vec<LocatedFn>,
) {
    for function in found {
        functions.push(LocatedFn {
            function,
            crate_name: crate_name.map(str::to_owned),
            join_key: join_key.map(str::to_owned),
        });
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
