//! Complexity metrics and source spans for Rust functions.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
mod cfg_filter;
mod cognitive;
mod cyclomatic;
mod visitor;

use cfg_filter::CfgUniverse;
use crap_core::{Error, FunctionComplexity, Metric, Result};
use std::path::Path;
use syn::parse::Parser;
use syn::visit::Visit;
use visitor::FunctionVisitor;

/// Reads `path` and returns every non-test function.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, or [`Error::Collect`]
/// if `syn` rejects the source.
pub fn analyze_file(
    path: &Path,
    metric: Metric,
    features: &[String],
) -> Result<Vec<FunctionComplexity>> {
    let source = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
    analyze_source_cfg(
        path,
        &source,
        metric,
        &CfgUniverse::new(features.iter().cloned()),
    )
}

/// Parses `source` as if it lived at `path`.
///
/// # Errors
///
/// Returns [`Error::Collect`] when the text is not valid Rust.
#[cfg(test)]
pub fn analyze_source(
    path: &Path,
    source: &str,
    metric: Metric,
) -> Result<Vec<FunctionComplexity>> {
    analyze_source_cfg(
        path,
        source,
        metric,
        &CfgUniverse::new(std::iter::empty::<String>()),
    )
}

/// Parses `source` with `features` treated as enabled for `#[cfg]`.
///
/// # Errors
///
/// Returns [`Error::Collect`] when the text is not valid Rust.
#[cfg(test)]
pub fn analyze_source_features(
    path: &Path,
    source: &str,
    metric: Metric,
    features: &[String],
) -> Result<Vec<FunctionComplexity>> {
    analyze_source_cfg(
        path,
        source,
        metric,
        &CfgUniverse::new(features.iter().cloned()),
    )
}

fn analyze_source_cfg(
    path: &Path,
    source: &str,
    metric: Metric,
    cfg: &CfgUniverse,
) -> Result<Vec<FunctionComplexity>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| Error::collect(format!("{}: {err}", path.display())))?;
    let mut visitor = FunctionVisitor {
        file: path,
        metric,
        out: Vec::new(),
        impl_type: None,
        cfg,
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

/// Parsed tokens from a function-like macro.
#[derive(Clone, Copy)]
enum ParsedMacro<'a> {
    /// The tokens formed one expression.
    Expr(&'a syn::Expr),
    /// The tokens formed a statement list.
    Stmts(&'a [syn::Stmt]),
    /// The tokens formed a file of items.
    File(&'a syn::File),
}

/// Owned parse of a macro body so the visitor can borrow it.
enum OwnedMacro {
    Expr(syn::Expr),
    Stmts(Vec<syn::Stmt>),
    File(syn::File),
}

/// Walks a parsed macro body with `visit`.
fn visit_parsed_macro(tokens: &proc_macro2::TokenStream, visit: impl FnOnce(ParsedMacro<'_>)) {
    let Some(owned) = parse_macro_body(tokens) else {
        return;
    };
    match &owned {
        OwnedMacro::Expr(expr) => visit(ParsedMacro::Expr(expr)),
        OwnedMacro::Stmts(stmts) => visit(ParsedMacro::Stmts(stmts)),
        OwnedMacro::File(file) => visit(ParsedMacro::File(file)),
    }
}

/// Best-effort parse of macro tokens as an expression, statements, or file.
fn parse_macro_body(tokens: &proc_macro2::TokenStream) -> Option<OwnedMacro> {
    if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
        return Some(OwnedMacro::Expr(expr));
    }
    if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
        return Some(OwnedMacro::File(file));
    }
    parse_stmt_seq(tokens.clone()).ok().map(OwnedMacro::Stmts)
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
