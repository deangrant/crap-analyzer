//! Complexity metrics and source spans for Rust functions.

mod cfg_filter;
mod cognitive;
mod cyclomatic;
mod visitor;

use crate::error::{Error, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};
use syn::parse::Parser;
use syn::visit::Visit;
use visitor::FunctionVisitor;

/// Which complexity metric to apply to each function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Metric {
    /// Cyclomatic complexity (one plus each decision point).
    Cyclomatic,
    /// Cognitive complexity (nesting-weighted control flow).
    Cognitive,
}

impl Metric {
    /// Default CRAP gate when `--threshold` is omitted.
    #[must_use]
    pub const fn default_threshold(self) -> f64 {
        match self {
            Self::Cyclomatic => 30.0,
            Self::Cognitive => 15.0,
        }
    }

    fn count(self, body: &syn::Block) -> usize {
        match self {
            Self::Cyclomatic => cyclomatic::count(body),
            Self::Cognitive => cognitive::count(body),
        }
    }
}

/// One function's complexity and inclusive line span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionComplexity {
    /// Source path as supplied to the walker.
    pub file: PathBuf,
    /// Free function name, or `Type::method` for impl and trait methods.
    pub name: String,
    /// One-based first line of the function.
    pub start_line: usize,
    /// One-based last line of the function body.
    pub end_line: usize,
    /// Selected metric value (cyclomatic minimum 1; cognitive may be 0).
    pub complexity: usize,
}

/// Reads `path` and returns every non-test function.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, or [`Error::Parse`]
/// if `syn` rejects the source.
pub fn analyze_file(path: &Path, metric: Metric) -> Result<Vec<FunctionComplexity>> {
    let source = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
    analyze_source(path, &source, metric)
}

/// Parses `source` as if it lived at `path`.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the text is not valid Rust.
pub fn analyze_source(
    path: &Path,
    source: &str,
    metric: Metric,
) -> Result<Vec<FunctionComplexity>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| Error::Parse(format!("{}: {err}", path.display())))?;
    let mut visitor = FunctionVisitor {
        file: path,
        metric,
        out: Vec::new(),
        impl_type: None,
        trait_name: None,
    };
    visitor.visit_file(&syntax);
    Ok(visitor.out)
}

/// Best-effort parse of macro tokens as an expression or statement list.
fn parse_macro_body(
    tokens: &proc_macro2::TokenStream,
) -> Option<(Option<syn::Expr>, Vec<syn::Stmt>)> {
    if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
        return Some((Some(expr), Vec::new()));
    }
    parse_stmt_seq(tokens.clone()).ok().map(|stmts| (None, stmts))
}

fn parse_stmt_seq(tokens: proc_macro2::TokenStream) -> syn::Result<Vec<syn::Stmt>> {
    (|input: syn::parse::ParseStream<'_>| {
        let mut stmts = Vec::new();
        while !input.is_empty() {
            stmts.push(input.parse()?);
        }
        Ok(stmts)
    })
    .parse2(tokens)
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "metric default thresholds are exact literals"
)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_differ_by_metric() {
        assert_eq!(Metric::Cyclomatic.default_threshold(), 30.0);
        assert_eq!(Metric::Cognitive.default_threshold(), 15.0);
    }
}
