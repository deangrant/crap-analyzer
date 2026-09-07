//! Language-agnostic CRAP scoring, LCOV join, and reporting.

pub mod coverage;
pub mod error;
pub mod language;
pub mod merge;
pub mod metric;
pub mod process;
pub mod report;
pub mod score;
pub mod threshold;

mod run;

#[doc(inline)]
pub use coverage::FileCoverage;
#[doc(inline)]
pub use error::{Error, Result};
#[doc(inline)]
pub use language::{Language, ReportFormat, ScanRequest, Target};
#[doc(inline)]
pub use merge::{
    CoverageJoin, CrapEntry, FunctionComplexity, LocatedFn, MissingPolicy, ambiguous_join_count,
    ambiguous_join_warning,
};
#[doc(inline)]
pub use metric::Metric;
#[doc(inline)]
pub use process::{
    HelpOrVersion, finish_run, help_or_version, print_core_err, print_ok, print_usage_err,
    reject_summary_json,
};
#[doc(inline)]
pub use report::render;
#[doc(inline)]
pub use run::{RunResult, run, run_with_coverage};
#[doc(inline)]
pub use score::Risk;
#[doc(inline)]
pub use threshold::{LENIENT, STRICT, parse_threshold};
