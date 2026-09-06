//! Complexity metrics and source spans for Rust functions.

mod cognitive;
mod cyclomatic;

use crate::error::{Error, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};
use syn::parse::Parser;
use syn::visit::{self, Visit};
use syn::{ImplItemFn, ItemFn, ItemImpl, ItemTrait, TraitItemFn};

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

fn has_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

fn is_test_item(attrs: &[syn::Attribute]) -> bool {
    has_attr(attrs, "test") || is_cfg_test(attrs)
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr.parse_args::<syn::Meta>().is_ok_and(|meta| cfg_is_test_only(&meta))
    })
}

fn cfg_is_test_only(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => {
            cfg_list(list).iter().any(cfg_is_test_only)
        }
        syn::Meta::List(list) if list.path.is_ident("any") => {
            let items = cfg_list(list);
            !items.is_empty() && items.iter().all(cfg_is_test_only)
        }
        _ => false,
    }
}

fn cfg_list(list: &syn::MetaList) -> Vec<syn::Meta> {
    list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .map(|items| items.into_iter().collect())
        .unwrap_or_default()
}

fn impl_type_name(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    tp.path.segments.last().map(|seg| seg.ident.to_string())
}

fn qualified_name(prefix: Option<&str>, method: &str) -> String {
    prefix.map_or_else(|| method.to_owned(), |ty| format!("{ty}::{method}"))
}

/// Best-effort parse of macro tokens as an expression or statement list.
pub fn parse_macro_body(
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

struct FunctionVisitor<'a> {
    file: &'a Path,
    metric: Metric,
    out: Vec<FunctionComplexity>,
    impl_type: Option<String>,
    trait_name: Option<String>,
}

impl FunctionVisitor<'_> {
    fn push_fn(&mut self, name: String, start_line: usize, end_line: usize, body: &syn::Block) {
        self.out.push(FunctionComplexity {
            file: self.file.to_path_buf(),
            name,
            start_line,
            end_line,
            complexity: self.metric.count(body),
        });
    }
}

impl<'ast> Visit<'ast> for FunctionVisitor<'_> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_test_item(&node.attrs) {
            return;
        }
        let start_line = node.sig.fn_token.span.start().line;
        let end_line = node.block.brace_token.span.close().end().line;
        self.push_fn(
            node.sig.ident.to_string(),
            start_line,
            end_line,
            &node.block,
        );
        visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let prev = self.impl_type.take();
        self.impl_type = impl_type_name(&node.self_ty);
        visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if is_test_item(&node.attrs) {
            return;
        }
        let method = node.sig.ident.to_string();
        let name = qualified_name(self.impl_type.as_deref(), &method);
        let start_line = node.sig.fn_token.span.start().line;
        let end_line = node.block.brace_token.span.close().end().line;
        self.push_fn(name, start_line, end_line, &node.block);
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let prev = self.trait_name.take();
        self.trait_name = Some(node.ident.to_string());
        visit::visit_item_trait(self, node);
        self.trait_name = prev;
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        if is_test_item(&node.attrs) {
            return;
        }
        if let Some(body) = &node.default {
            let method = node.sig.ident.to_string();
            let name = qualified_name(self.trait_name.as_deref(), &method);
            let start_line = node.sig.fn_token.span.start().line;
            let end_line = body.brace_token.span.close().end().line;
            self.push_fn(name, start_line, end_line, body);
        }
        visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if !is_cfg_test(&node.attrs) {
            visit::visit_item_mod(self, node);
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "metric default thresholds are exact literals"
)]
mod tests {
    use super::*;

    fn snippet(src: &str, metric: Metric) -> Vec<FunctionComplexity> {
        let parsed = analyze_source(Path::new("t.rs"), src, metric);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default()
    }

    fn cyclo(src: &str) -> Vec<FunctionComplexity> {
        snippet(src, Metric::Cyclomatic)
    }

    #[test]
    fn test_functions_are_skipped() {
        let fns = cyclo("#[test] fn t() { if true {} } fn keep() {}");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn cfg_test_helper_is_skipped() {
        let fns = cyclo("#[cfg(test)] fn helper() { if true {} } fn keep() {}");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn cfg_test_impl_is_skipped() {
        let src = "struct Foo; #[cfg(test)] impl Foo { fn bar(&self) { if true {} } }";
        assert!(cyclo(src).is_empty());
    }

    #[test]
    fn cfg_all_test_is_skipped() {
        let src = "#[cfg(all(test, feature = \"x\"))] fn helper() { if true {} } fn keep() {}";
        let fns = cyclo(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn cfg_any_test_or_unix_is_kept() {
        let fns = cyclo("#[cfg(any(test, unix))] fn f() { if true {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }

    #[test]
    fn cfg_test_mod_is_skipped() {
        let src = "#[cfg(test)] mod tests { fn helper() { if true {} } } fn keep() {}";
        let fns = cyclo(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn impl_methods_are_prefixed() {
        let src = "struct Foo; impl Foo { fn bar(&self) { if true {} } }";
        let fns = cyclo(src);
        assert_eq!(fns[0].name, "Foo::bar");
        assert_eq!(fns[0].complexity, 2);
    }

    #[test]
    fn trait_default_is_scored() {
        let fns = cyclo("trait T { fn m(&self) { if true {} } }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "T::m");
        assert_eq!(fns[0].complexity, 2);
    }

    #[test]
    fn nested_fn_is_scored_separately() {
        let fns = cyclo("fn outer() { fn inner() { if true {} } }");
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "outer");
        assert_eq!(fns[0].complexity, 1);
        assert_eq!(fns[1].name, "inner");
        assert_eq!(fns[1].complexity, 2);
    }

    #[test]
    fn qualified_name_uses_prefix_when_present() {
        assert_eq!(qualified_name(Some("Foo"), "bar"), "Foo::bar");
        assert_eq!(qualified_name(None, "bar"), "bar");
    }

    #[test]
    fn default_thresholds_differ_by_metric() {
        assert_eq!(Metric::Cyclomatic.default_threshold(), 30.0);
        assert_eq!(Metric::Cognitive.default_threshold(), 15.0);
    }

    #[test]
    fn cfg_any_test_only_is_skipped() {
        let fns = cyclo("#[cfg(any(test))] fn helper() { if true {} } fn keep() {}");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn cfg_feature_is_kept() {
        let fns = cyclo("#[cfg(feature = \"x\")] fn f() { if true {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }

    #[test]
    fn nested_mod_functions_are_scored() {
        let fns = cyclo("mod inner { fn f() { if true {} } }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }

    #[test]
    fn cfg_test_trait_is_skipped() {
        let src = "#[cfg(test)] trait T { fn m(&self) { if true {} } }";
        assert!(cyclo(src).is_empty());
    }

    #[test]
    fn test_impl_method_is_skipped() {
        let src = "struct Foo; impl Foo { #[test] fn t() { if true {} } fn keep(&self) {} }";
        let fns = cyclo(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "Foo::keep");
    }

    #[test]
    fn trait_declaration_without_default_is_skipped() {
        let fns = cyclo("trait T { fn m(&self); }");
        assert!(fns.is_empty());
    }

    #[test]
    fn test_trait_method_is_skipped() {
        let src = "trait T { #[test] fn t(&self) { if true {} } fn keep(&self) {} }";
        let fns = cyclo(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "T::keep");
    }

    #[test]
    fn tuple_impl_method_is_unprefixed() {
        let fns = cyclo("impl (u8, u8) { fn m() {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "m");
    }
}
