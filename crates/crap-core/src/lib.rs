//! Language-agnostic CRAP scoring, LCOV join, and reporting.

pub mod coverage;
pub mod error;
pub mod language;
pub mod merge;
pub mod metric;
pub mod report;
pub mod score;

mod run;

#[doc(inline)]
pub use coverage::FileCoverage;
#[doc(inline)]
pub use error::{Error, Result};
#[doc(inline)]
pub use language::{Language, ReportFormat, ScanRequest, Target};
#[doc(inline)]
pub use merge::{CrapEntry, FunctionComplexity, LocatedFn, MissingPolicy};
#[doc(inline)]
pub use metric::Metric;
#[doc(inline)]
pub use report::render;
#[doc(inline)]
pub use run::{RunResult, run, run_with_coverage};
#[doc(inline)]
pub use score::Risk;
