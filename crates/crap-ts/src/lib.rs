//! TypeScript frontend: discover packages and collect function complexity.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
pub mod cli;
pub(crate) mod complexity;
pub(crate) mod project_resolve;
pub(crate) mod walk;

use crap_core::{Error, Language, LocatedFn, Metric, Result, ScanRequest, Target};
use project_resolve::Package;
use std::path::{Component, Path};

/// TypeScript implementation of [`Language`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsLanguage {
    /// Analyze every package under the workspace root.
    pub workspace: bool,
    /// Selected package names (`-p`).
    pub packages: Vec<String>,
}

impl Language for TsLanguage {
    fn resolve_targets(&self, request: &ScanRequest) -> Result<Vec<Target>> {
        if self.uses_packages() {
            return self.targets_from_selection(request);
        }
        if let Some(targets) = Self::targets_from_package_root(&request.path)? {
            return Ok(targets);
        }
        Ok(vec![Self::path_target(request)])
    }

    fn collect_functions(&self, targets: &[Target], metric: Metric) -> Result<Vec<LocatedFn>> {
        collect_functions(targets, metric)
    }
}

impl TsLanguage {
    const fn uses_packages(&self) -> bool {
        self.workspace || !self.packages.is_empty()
    }

    fn targets_from_packages(packages: &[Package], workspace_root: &Path) -> Vec<Target> {
        packages
            .iter()
            .map(|pkg| Target {
                root: pkg.root.clone(),
                crate_name: Some(pkg.name.clone()),
                join_key: Some(relative_join_key(workspace_root, &pkg.root)),
                skip: project_resolve::nested_package_roots(&pkg.root, packages),
                enabled_features: Vec::new(),
            })
            .collect()
    }

    fn targets_from_selection(&self, request: &ScanRequest) -> Result<Vec<Target>> {
        let packages = if self.workspace {
            project_resolve::all_packages(&request.path)?
        } else {
            project_resolve::selected_packages(&self.packages, &request.path)?
        };
        Ok(Self::targets_from_packages(&packages, &request.path))
    }

    fn targets_from_package_root(path: &Path) -> Result<Option<Vec<Target>>> {
        if !path.join("package.json").is_file() {
            return Ok(None);
        }
        let package = project_resolve::root_package(path)?;
        Ok(Some(Self::targets_from_packages(&[package], path)))
    }

    fn path_target(request: &ScanRequest) -> Target {
        Target {
            root: request.path.clone(),
            crate_name: None,
            join_key: None,
            skip: Vec::new(),
            enabled_features: Vec::new(),
        }
    }
}

/// Builds a relative POSIX join key for `pkg_root` under `workspace_root`.
fn relative_join_key(workspace_root: &Path, pkg_root: &Path) -> String {
    let rel = pkg_root.strip_prefix(workspace_root).unwrap_or(pkg_root);
    let key = posix_components(rel);
    if key.is_empty() {
        pkg_root
            .file_name()
            .map_or_else(|| ".".into(), |n| n.to_string_lossy().into_owned())
    } else {
        key
    }
}

fn posix_components(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn collect_functions(targets: &[Target], metric: Metric) -> Result<Vec<LocatedFn>> {
    let mut functions = Vec::new();
    let mut details = Vec::new();
    let (succeeded, failed) = collect_targets(targets, metric, &mut functions, &mut details)?;
    if failed > 0 {
        return Err(Error::collect(collect_failure(
            "TypeScript",
            failed,
            succeeded,
            &details,
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
    let files = walk::ts_files(&target.root, &target.skip)?;
    for file in files {
        if take_file(
            &file,
            target.crate_name.as_deref(),
            target.join_key.as_deref(),
            metric,
            functions,
            details,
        ) {
            *succeeded += 1;
        } else {
            *failed += 1;
        }
    }
    Ok(())
}

fn take_file(
    file: &Path,
    crate_name: Option<&str>,
    join_key: Option<&str>,
    metric: Metric,
    functions: &mut Vec<LocatedFn>,
    details: &mut Vec<String>,
) -> bool {
    let Some(source) = read_source(file, details) else {
        return false;
    };
    analyze_into(
        file, &source, crate_name, join_key, metric, functions, details,
    )
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
    join_key: Option<&str>,
    metric: Metric,
    functions: &mut Vec<LocatedFn>,
    details: &mut Vec<String>,
) -> bool {
    match complexity::analyze_source(file, source, metric) {
        Ok(found) => {
            for function in found {
                functions.push(LocatedFn {
                    function,
                    crate_name: crate_name.map(str::to_owned),
                    join_key: join_key.map(str::to_owned),
                });
            }
            true
        }
        Err(err) => {
            details.push(format!("skipping {}: {err}", file.display()));
            false
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
