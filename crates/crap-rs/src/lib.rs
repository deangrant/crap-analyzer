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
    if failed > 0 && succeeded == 0 {
        return Err(Error::collect(format!(
            "failed to parse all {failed} Rust file(s)"
        )));
    }
    Ok((functions, warnings))
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
mod tests {
    use super::*;
    use crap_core::ReportFormat;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn feature_pkg(pairs: &[(&str, &[&str])]) -> Package {
        let mut features = BTreeMap::new();
        for (name, deps) in pairs {
            features.insert(
                (*name).to_owned(),
                deps.iter().map(|dep| (*dep).to_owned()).collect(),
            );
        }
        Package {
            name: "demo".into(),
            root: PathBuf::from("/demo"),
            features,
        }
    }

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
            format: ReportFormat::Text,
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
        let pkg = feature_pkg(&[("std", &[]), ("serde", &[])]);
        assert_eq!(
            lang.enabled_features(Some(&pkg)),
            vec!["serde".to_owned(), "std".to_owned()]
        );
        let cli_only = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection {
                features: vec!["cli".into()],
                all_features: true,
                no_default_features: false,
            },
        };
        assert_eq!(cli_only.enabled_features(None), vec!["cli".to_owned()]);
    }

    #[test]
    fn default_features_close_transitively() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection::default(),
        };
        let pkg = feature_pkg(&[("default", &["std"]), ("std", &["serde"]), ("serde", &[])]);
        assert_eq!(
            lang.enabled_features(Some(&pkg)),
            vec!["serde".to_owned(), "std".to_owned()]
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
            format: ReportFormat::Text,
        };
        let targets = lang.resolve_targets(&request).unwrap_or_default();
        assert_eq!(targets[0].enabled_features, vec!["serde".to_owned()]);
    }

    #[test]
    fn collect_targets_propagates_walk_error() {
        let targets = [Target {
            root: PathBuf::from("/no/such/crap-rs-collect-targets"),
            crate_name: None,
            skip: Vec::new(),
            enabled_features: Vec::new(),
        }];
        let mut functions = Vec::new();
        let mut warnings = Vec::new();
        let result = collect_targets(&targets, Metric::Cyclomatic, &mut functions, &mut warnings);
        assert!(result.is_err());
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
            format: ReportFormat::Text,
        };
        let targets = lang.resolve_targets(&request);
        assert!(targets.is_ok(), "{targets:?}");
        let targets = targets.unwrap_or_default();
        let names: Vec<&str> =
            targets.iter().filter_map(|target| target.crate_name.as_deref()).collect();
        assert!(names.len() >= 2 && names.contains(&"crap-core") && names.contains(&"crap-rs"));
    }

    #[test]
    fn member_path_selects_only_that_package() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection::default(),
        };
        let member = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let request = ScanRequest {
            path: member,
            lcov: PathBuf::from("lcov.info"),
            metric: Metric::Cyclomatic,
            threshold: None,
            summary: false,
            fail_above: false,
            missing: crap_core::MissingPolicy::Pessimistic,
            format: ReportFormat::Text,
        };
        let targets = lang.resolve_targets(&request);
        assert!(targets.is_ok(), "{targets:?}");
        let targets = targets.unwrap_or_default();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].crate_name.as_deref(), Some("crap-rs"));
    }

    #[test]
    fn broken_manifest_is_a_resolve_error() {
        let lang = RustLanguage {
            workspace: false,
            packages: Vec::new(),
            features: FeatureSelection::default(),
        };
        let dir = std::env::temp_dir().join(format!(
            "crap-rs-bad-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let created = std::fs::create_dir_all(&dir);
        assert!(created.is_ok(), "{created:?}");
        let written = std::fs::write(dir.join("Cargo.toml"), "this is not a manifest\n");
        assert!(written.is_ok(), "{written:?}");
        let request = ScanRequest {
            path: dir.clone(),
            lcov: PathBuf::from("lcov.info"),
            metric: Metric::Cyclomatic,
            threshold: None,
            summary: false,
            fail_above: false,
            missing: crap_core::MissingPolicy::Pessimistic,
            format: ReportFormat::Text,
        };
        let targets = lang.resolve_targets(&request);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(targets.is_err(), "{targets:?}");
    }
}
