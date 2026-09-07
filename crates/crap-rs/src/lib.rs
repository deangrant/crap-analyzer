//! Rust frontend: discover Cargo targets and collect function complexity.

pub mod cli;
pub(crate) mod complexity;
pub(crate) mod walk;
pub(crate) mod workspace;

use crap_core::{Error, Language, LocatedFn, Metric, Result, ScanRequest, Target};
use std::path::Path;
use workspace::Package;

/// Rust implementation of [`Language`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustLanguage {
    /// Analyze every workspace member.
    pub workspace: bool,
    /// Selected package names (`-p`).
    pub packages: Vec<String>,
    /// Cargo features treated as enabled for `#[cfg]`.
    pub features: FeatureSelection,
}

/// Cargo feature flags used when evaluating `#[cfg]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeatureSelection {
    /// Extra features from `--features`.
    pub features: Vec<String>,
    /// Enable every named package feature.
    pub all_features: bool,
    /// Do not enable the package `default` feature list.
    pub no_default_features: bool,
}

impl Language for RustLanguage {
    fn resolve_targets(&self, request: &ScanRequest) -> Result<Vec<Target>> {
        if self.uses_packages() {
            return self.targets_from_selection(request);
        }
        if let Some(targets) = self.targets_from_manifest_root(&request.path)? {
            return Ok(targets);
        }
        Ok(vec![self.path_target(request)])
    }

    fn collect_functions(
        &self,
        targets: &[Target],
        metric: Metric,
    ) -> Result<(Vec<LocatedFn>, Vec<String>)> {
        collect_functions(targets, metric)
    }
}

impl RustLanguage {
    const fn uses_packages(&self) -> bool {
        self.workspace || !self.packages.is_empty()
    }

    fn enabled_features(&self, pkg: Option<&Package>) -> Vec<String> {
        let Some(pkg) = pkg else {
            let mut enabled = self.features.features.clone();
            enabled.sort();
            enabled.dedup();
            return enabled;
        };
        workspace::close_features(&pkg.features, &self.feature_seeds(pkg))
    }

    fn feature_seeds(&self, pkg: &Package) -> Vec<String> {
        if self.features.all_features {
            return pkg.features.keys().filter(|key| *key != "default").cloned().collect();
        }
        let mut seeds = Vec::new();
        if !self.features.no_default_features
            && let Some(default) = pkg.features.get("default")
        {
            seeds.extend(default.iter().cloned());
        }
        seeds.extend(self.features.features.iter().cloned());
        seeds
    }

    fn targets_from_packages(&self, packages: &[Package]) -> Vec<Target> {
        packages
            .iter()
            .map(|pkg| Target {
                root: pkg.root.clone(),
                crate_name: Some(pkg.name.clone()),
                skip: workspace::nested_member_roots(&pkg.root, packages),
                enabled_features: self.enabled_features(Some(pkg)),
            })
            .collect()
    }

    fn targets_from_selection(&self, request: &ScanRequest) -> Result<Vec<Target>> {
        let packages = if self.workspace {
            workspace::all_members(&request.path)?
        } else {
            workspace::selected_members(&self.packages, &request.path)?
        };
        Ok(self.targets_from_packages(&packages))
    }

    fn targets_from_manifest_root(&self, path: &Path) -> Result<Option<Vec<Target>>> {
        if !path.join("Cargo.toml").is_file() {
            return Ok(None);
        }
        let packages = workspace::packages_for_path(path)?;
        Ok(Some(self.targets_from_packages(&packages)))
    }

    fn path_target(&self, request: &ScanRequest) -> Target {
        Target {
            root: request.path.clone(),
            crate_name: None,
            skip: Vec::new(),
            enabled_features: self.enabled_features(None),
        }
    }
}

fn collect_functions(targets: &[Target], metric: Metric) -> Result<(Vec<LocatedFn>, Vec<String>)> {
    let mut functions = Vec::new();
    let mut warnings = Vec::new();
    let (succeeded, failed) = collect_targets(targets, metric, &mut functions, &mut warnings)?;
    if failed > 0 {
        return Err(Error::collect(collect_failure(
            "Rust", failed, succeeded, &warnings,
        )));
    }
    Ok((functions, warnings))
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
    warnings: &mut Vec<String>,
) -> Result<(usize, usize)> {
    let mut succeeded = 0_usize;
    let mut failed = 0_usize;
    for target in targets {
        collect_target(
            target,
            metric,
            functions,
            warnings,
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
    warnings: &mut Vec<String>,
    succeeded: &mut usize,
    failed: &mut usize,
) -> Result<()> {
    let files = walk::rust_files(&target.root, &target.skip)?;
    for file in files {
        if take_file(
            &file,
            target.crate_name.as_deref(),
            metric,
            &target.enabled_features,
            functions,
            warnings,
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
    metric: Metric,
    features: &[String],
    functions: &mut Vec<LocatedFn>,
    warnings: &mut Vec<String>,
) -> bool {
    match complexity::analyze_file(file, metric, features) {
        Ok(found) => {
            for function in found {
                functions.push(LocatedFn {
                    function,
                    crate_name: crate_name.map(str::to_owned),
                });
            }
            true
        }
        Err(err) => {
            warnings.push(parse_warning(file, &err));
            false
        }
    }
}

fn parse_warning(path: &Path, err: &Error) -> String {
    format!("skipping {}: {err}", path.display())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
