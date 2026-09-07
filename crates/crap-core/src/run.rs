//! Drive the language-agnostic analysis pipeline.

use crate::coverage::{self, FileCoverage};
use crate::error::Result;
use crate::language::{Language, ScanRequest};
use crate::merge::{CrapEntry, join};
use crate::score::exceeds_threshold;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::PathBuf;

/// Finished analysis ready to print.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunResult {
    /// Functions after join, scored and sorted worst-first.
    pub entries: Vec<CrapEntry>,
    /// Non-fatal notices that survived a successful collect.
    pub warnings: Vec<String>,
    /// True when `--fail-above` should trip.
    pub gate_failed: bool,
}

/// Parses LCOV from `request.coverage`, then runs the join and gate.
///
/// # Errors
///
/// Returns I/O, metadata, usage, or collect errors. Any source file
/// that fails to parse fails the run.
pub fn run<L: Language>(lang: &L, request: &ScanRequest) -> Result<RunResult> {
    let coverage = coverage::parse_lcov(&request.coverage)?;
    run_with_coverage(lang, request, &coverage)
}

/// Resolves targets, collects functions, joins coverage, and applies the gate.
///
/// Same pipeline as [`run`] after coverage has already been parsed.
///
/// # Errors
///
/// Returns metadata, usage, or collect errors. Any source file that fails
/// to parse fails the run.
pub fn run_with_coverage<L: Language, S: BuildHasher>(
    lang: &L,
    request: &ScanRequest,
    coverage: &HashMap<PathBuf, FileCoverage, S>,
) -> Result<RunResult> {
    let targets = lang.resolve_targets(request)?;
    let (functions, warnings) = lang.collect_functions(&targets, request.metric)?;
    let entries = join(&functions, coverage, request.missing);
    let threshold = request.effective_threshold();
    let gate_failed =
        request.fail_above && entries.iter().any(|entry| exceeds_threshold(entry.crap, threshold));
    Ok(RunResult {
        entries,
        warnings,
        gate_failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::language::{ReportFormat, Target};
    use crate::merge::{FunctionComplexity, LocatedFn, MissingPolicy};
    use crate::metric::Metric;
    use crate::report::render;
    use std::path::{Path, PathBuf};

    struct FakeLang {
        functions: Vec<LocatedFn>,
        warnings: Vec<String>,
        fail: bool,
    }

    impl Language for FakeLang {
        fn resolve_targets(&self, request: &ScanRequest) -> Result<Vec<Target>> {
            Ok(vec![Target {
                root: request.path.clone(),
                crate_name: Some("demo".into()),
                skip: Vec::new(),
                enabled_features: Vec::new(),
            }])
        }

        fn collect_functions(
            &self,
            _targets: &[Target],
            _metric: Metric,
        ) -> Result<(Vec<LocatedFn>, Vec<String>)> {
            if self.fail {
                return Err(Error::collect("failed to parse all 1 file(s)"));
            }
            Ok((self.functions.clone(), self.warnings.clone()))
        }
    }

    fn request(
        coverage: &Path,
        summary: bool,
        fail_above: bool,
        threshold: Option<f64>,
    ) -> ScanRequest {
        ScanRequest {
            path: PathBuf::from("."),
            coverage: coverage.to_path_buf(),
            metric: Metric::Cyclomatic,
            threshold,
            summary,
            fail_above,
            missing: MissingPolicy::Pessimistic,
            format: ReportFormat::Text,
        }
    }

    fn func(file: &str, name: &str, start: usize, end: usize, complexity: usize) -> LocatedFn {
        LocatedFn {
            function: FunctionComplexity {
                file: PathBuf::from(file),
                name: name.into(),
                start_line: start,
                end_line: end,
                complexity,
            },
            crate_name: Some("demo".into()),
        }
    }

    fn write_lcov(dir: &Path) -> PathBuf {
        let path = dir.join("lcov.info");
        let written = std::fs::write(&path, "TN:\nSF:src/lib.rs\nDA:1,1\nDA:2,1\nend_of_record\n");
        assert!(written.is_ok(), "{written:?}");
        path
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "crap-core-run-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let created = std::fs::create_dir_all(&dir);
        assert!(created.is_ok(), "{created:?}");
        dir
    }

    fn require_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
        result: std::result::Result<T, E>,
    ) -> T {
        assert!(result.is_ok(), "{result:?}");
        result.unwrap_or_default()
    }

    #[test]
    fn run_scores_and_trips_the_gate() {
        let dir = temp_dir();
        let coverage = write_lcov(&dir);
        let lang = FakeLang {
            functions: vec![func("src/lib.rs", "dense", 1, 2, 31)],
            warnings: vec!["skipping broken.rs: parse".into()],
            fail: false,
        };
        let req = request(&coverage, false, true, Some(8.0));
        let result = require_ok(run(&lang, &req));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.gate_failed);
        assert_eq!((result.warnings.len(), result.entries.len()), (1, 1));
        let table = require_ok(render(&req, &result, "rust", false));
        assert!(table.contains("FAIL") && table.contains("dense"));
    }

    #[test]
    fn render_summary_without_crates() {
        let result = RunResult {
            entries: vec![CrapEntry {
                file: PathBuf::from("src/lib.rs"),
                function: "okfn".into(),
                start_line: 1,
                end_line: 1,
                complexity: 1,
                coverage: 100.0,
                crap: 1.0,
                crate_name: None,
            }],
            warnings: Vec::new(),
            gate_failed: false,
        };
        let req = request(Path::new("lcov.info"), true, false, Some(30.0));
        let summary = render(&req, &result, "rust", false);
        assert!(summary.is_ok(), "{summary:?}");
        let summary = summary.unwrap_or_default();
        assert!(summary.contains("1 functions, 0 exceed threshold"));
        assert!(!summary.contains("demo:"));
    }

    #[test]
    fn render_summary_lists_crates() {
        let result = RunResult {
            entries: vec![CrapEntry {
                file: PathBuf::from("src/lib.rs"),
                function: "okfn".into(),
                start_line: 1,
                end_line: 1,
                complexity: 1,
                coverage: 100.0,
                crap: 1.0,
                crate_name: Some("demo".into()),
            }],
            warnings: Vec::new(),
            gate_failed: false,
        };
        let req = request(Path::new("lcov.info"), true, false, Some(30.0));
        let summary = render(&req, &result, "rust", false);
        assert!(summary.is_ok(), "{summary:?}");
        let summary = summary.unwrap_or_default();
        assert!(summary.contains("demo: 1 functions, 0 over"));
        assert!(!summary.contains("FUNCTION"));
    }

    #[test]
    fn render_json_ignores_summary() {
        let result = RunResult {
            entries: vec![CrapEntry {
                file: PathBuf::from("src/lib.rs"),
                function: "okfn".into(),
                start_line: 1,
                end_line: 4,
                complexity: 1,
                coverage: 100.0,
                crap: 1.0,
                crate_name: None,
            }],
            warnings: Vec::new(),
            gate_failed: false,
        };
        let mut req = request(Path::new("lcov.info"), true, false, Some(15.0));
        req.format = ReportFormat::Json;
        let json = render(&req, &result, "rust", false);
        assert!(json.is_ok(), "{json:?}");
        let json = json.unwrap_or_default();
        assert!(json.contains("\"schema_version\""));
        assert!(json.contains("\"functions\""));
        assert!(!json.contains("exceed threshold"));
    }

    #[test]
    fn collect_error_is_propagated() {
        let dir = temp_dir();
        let coverage = write_lcov(&dir);
        let lang = FakeLang {
            functions: Vec::new(),
            warnings: Vec::new(),
            fail: true,
        };
        let result = run(&lang, &request(&coverage, false, false, None));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err());
    }

    #[test]
    fn missing_coverage_is_io_error() {
        let lang = FakeLang {
            functions: Vec::new(),
            warnings: Vec::new(),
            fail: false,
        };
        let result = run(
            &lang,
            &request(Path::new("/no/such/crap-core.lcov"), false, false, None),
        );
        assert!(result.is_err());
    }
}
