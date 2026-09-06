//! Complexity metrics and source spans for Rust functions.

mod cfg_filter;
mod cognitive;
mod cyclomatic;
mod visitor;

use crap_core::{Error, FunctionComplexity, Metric, Result};
use std::path::Path;
use syn::parse::Parser;
use syn::visit::Visit;
use visitor::FunctionVisitor;

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

fn count_metric(metric: Metric, body: &syn::Block) -> usize {
    match metric {
        Metric::Cyclomatic => cyclomatic::count(body),
        Metric::Cognitive => cognitive::count(body),
    }
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
