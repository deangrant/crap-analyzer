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
}

impl Language for RustLanguage {
    fn resolve_targets(&self, request: &ScanRequest) -> Result<Vec<Target>> {
        if !self.uses_packages() {
            return Ok(vec![Target {
                root: request.path.clone(),
                crate_name: None,
                skip: Vec::new(),
            }]);
        }
        let packages = if self.workspace {
            workspace::all_members(&request.path)?
        } else {
            workspace::selected_members(&self.packages, &request.path)?
        };
        Ok(targets_from_packages(&packages))
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

fn collect_functions(targets: &[Target], metric: Metric) -> Result<(Vec<LocatedFn>, Vec<String>)> {
    let mut functions = Vec::new();
    let mut warnings = Vec::new();
    let mut succeeded = 0_usize;
    let mut failed = 0_usize;
    for target in targets {
        let files = walk::rust_files(&target.root, &target.skip)?;
        for file in files {
            if take_file(
                &file,
                target.crate_name.as_deref(),
                metric,
                &mut functions,
                &mut warnings,
            ) {
                succeeded += 1;
            } else {
                failed += 1;
            }
        }
    }
    if failed > 0 && succeeded == 0 {
        return Err(Error::Parse(format!(
            "failed to parse all {failed} Rust file(s)"
        )));
    }
    Ok((functions, warnings))
}

fn take_file(
    file: &Path,
    crate_name: Option<&str>,
    metric: Metric,
    functions: &mut Vec<LocatedFn>,
    warnings: &mut Vec<String>,
) -> bool {
    match complexity::analyze_file(file, metric) {
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
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn path_mode_uses_the_request_root() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
        };
        let request = ScanRequest {
            path: PathBuf::from("/proj"),
            lcov: PathBuf::from("lcov.info"),
            metric: Metric::Cyclomatic,
            threshold: None,
            summary: false,
            fail_above: false,
            missing: crap_core::MissingPolicy::Pessimistic,
        };
        let targets = lang.resolve_targets(&request);
        assert!(targets.is_ok(), "{targets:?}");
        let targets = targets.unwrap_or_default();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].root, PathBuf::from("/proj"));
        assert!(targets[0].crate_name.is_none());
    }

    #[test]
    fn parse_warning_includes_path() {
        let warning = parse_warning(Path::new("broken.rs"), &Error::Parse("nope".into()));
        assert!(warning.contains("broken.rs"));
        assert!(warning.contains("nope"));
    }
}
