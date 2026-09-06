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
            let packages = if self.workspace {
                workspace::all_members(&request.path)?
            } else {
                workspace::selected_members(&self.packages, &request.path)?
            };
            return Ok(self.targets_from_packages(&packages));
        }
        if request.path.join("Cargo.toml").is_file()
            && let Ok(packages) = workspace::all_members(&request.path)
        {
            return Ok(self.targets_from_packages(&packages));
        }
        Ok(vec![Target {
            root: request.path.clone(),
            crate_name: None,
            skip: Vec::new(),
            enabled_features: self.enabled_features(None),
        }])
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
        if self.features.all_features {
            return pkg.map_or_else(
                || self.features.features.clone(),
                |p| p.all_features.clone(),
            );
        }
        let mut enabled = Vec::new();
        if !self.features.no_default_features
            && let Some(pkg) = pkg
        {
            enabled.extend(pkg.default_features.iter().cloned());
        }
        enabled.extend(self.features.features.iter().cloned());
        enabled.sort();
        enabled.dedup();
        enabled
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
                &target.enabled_features,
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
        return Err(Error::collect(format!(
            "failed to parse all {failed} Rust file(s)"
        )));
    }
    Ok((functions, warnings))
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
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn path_mode_uses_the_request_root() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection::default(),
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
    fn all_features_uses_package_names() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection {
                features: Vec::new(),
                all_features: true,
                no_default_features: false,
            },
        };
        let pkg = Package {
            name: "demo".into(),
            root: PathBuf::from("/demo"),
            default_features: vec!["std".into()],
            all_features: vec!["std".into(), "serde".into()],
        };
        assert_eq!(
            lang.enabled_features(Some(&pkg)),
            vec!["std".to_owned(), "serde".to_owned()]
        );
    }

    #[test]
    fn path_mode_uses_explicit_features() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection {
                features: vec!["serde".into()],
                all_features: false,
                no_default_features: false,
            },
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
        let targets = lang.resolve_targets(&request).unwrap_or_default();
        assert_eq!(targets[0].enabled_features, vec!["serde".to_owned()]);
    }

    #[test]
    fn parse_warning_includes_path() {
        let warning = parse_warning(Path::new("broken.rs"), &Error::collect("nope"));
        assert!(warning.contains("broken.rs"));
        assert!(warning.contains("nope"));
    }

    #[test]
    fn workspace_root_isolates_members_without_workspace_flag() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection::default(),
        };
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let request = ScanRequest {
            path: workspace,
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
        assert!(targets.len() >= 2, "{targets:?}");
        assert!(targets.iter().all(|target| target.crate_name.is_some()));
        let names: Vec<&str> =
            targets.iter().filter_map(|target| target.crate_name.as_deref()).collect();
        assert!(names.contains(&"crap-core"), "{names:?}");
        assert!(names.contains(&"crap-rs"), "{names:?}");
    }
}
