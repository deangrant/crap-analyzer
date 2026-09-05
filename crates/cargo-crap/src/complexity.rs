//! Cyclomatic complexity and source spans for Rust functions.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use syn::parse::Parser;
use syn::visit::{self, Visit};
use syn::{BinOp, ImplItemFn, ItemFn, ItemImpl, ItemTrait, TraitItemFn};

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
    /// Cyclomatic complexity, minimum 1.
    pub cyclomatic: usize,
}

/// Reads `path` and returns every non-test function.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, or [`Error::Parse`]
/// if `syn` rejects the source.
pub fn analyze_file(path: &Path) -> Result<Vec<FunctionComplexity>> {
    let source = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
    analyze_source(path, &source)
}

/// Parses `source` as if it lived at `path`.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the text is not valid Rust.
fn analyze_source(path: &Path, source: &str) -> Result<Vec<FunctionComplexity>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| Error::Parse(format!("{}: {err}", path.display())))?;
    let mut visitor = FunctionVisitor {
        file: path,
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

struct FunctionVisitor<'a> {
    file: &'a Path,
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
            cyclomatic: count_cyclomatic(body),
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
        let name = match &self.impl_type {
            Some(ty) => format!("{ty}::{method}"),
            None => method,
        };
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
            let name = match &self.trait_name {
                Some(tr) => format!("{tr}::{method}"),
                None => method,
            };
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

fn count_cyclomatic(body: &syn::Block) -> usize {
    let mut counter = CcCounter { count: 1 };
    counter.visit_block(body);
    counter.count
}

struct CcCounter {
    count: usize,
}

impl<'ast> Visit<'ast> for CcCounter {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.count += 1;
        visit::visit_expr_if(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.count += 1;
        visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.count += 1;
        visit::visit_expr_while(self, node);
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.count += 1;
        visit::visit_expr_loop(self, node);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.count += 1;
        visit::visit_arm(self, node);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(node.op, BinOp::And(_) | BinOp::Or(_)) {
            self.count += 1;
        }
        visit::visit_expr_binary(self, node);
    }

    fn visit_expr_try(&mut self, node: &'ast syn::ExprTry) {
        self.count += 1;
        visit::visit_expr_try(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.visit_macro_tokens(&node.tokens);
    }

    fn visit_item(&mut self, _node: &'ast syn::Item) {}
}

impl CcCounter {
    fn visit_macro_tokens(&mut self, tokens: &proc_macro2::TokenStream) {
        if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
            self.visit_expr(&expr);
            return;
        }
        let Ok(stmts) = parse_stmt_seq(tokens.clone()) else {
            return;
        };
        for stmt in &stmts {
            self.visit_stmt(stmt);
        }
    }
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
mod tests {
    use super::*;
    use std::path::Path;

    fn snippet(src: &str) -> Vec<FunctionComplexity> {
        let parsed = analyze_source(Path::new("t.rs"), src);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default()
    }

    #[test]
    fn straight_line_is_one() {
        let fns = snippet("fn trivial() { let x = 1; }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].cyclomatic, 1);
        assert_eq!(fns[0].name, "trivial");
    }

    #[test]
    fn if_adds_one() {
        let fns = snippet("fn f(x: i32) { if x > 0 { x; } }");
        assert_eq!(fns[0].cyclomatic, 2);
    }

    #[test]
    fn and_adds_one() {
        let fns = snippet("fn f(a: bool, b: bool) { let _ = a && b; }");
        assert_eq!(fns[0].cyclomatic, 2);
    }

    #[test]
    fn three_match_arms_add_three() {
        let src = "fn f(x: i32) { match x { 0 => {}, 1 => {}, _ => {} } }";
        assert_eq!(snippet(src)[0].cyclomatic, 4);
    }

    #[test]
    fn test_functions_are_skipped() {
        let fns = snippet("#[test] fn t() { if true {} } fn keep() {}");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn cfg_test_helper_is_skipped() {
        let fns = snippet("#[cfg(test)] fn helper() { if true {} } fn keep() {}");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn cfg_test_impl_is_skipped() {
        let src = "struct Foo; #[cfg(test)] impl Foo { fn bar(&self) { if true {} } }";
        assert!(snippet(src).is_empty());
    }

    #[test]
    fn cfg_all_test_is_skipped() {
        let src = "#[cfg(all(test, feature = \"x\"))] fn helper() { if true {} } fn keep() {}";
        let fns = snippet(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn cfg_any_test_or_unix_is_kept() {
        let fns = snippet("#[cfg(any(test, unix))] fn f() { if true {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }

    #[test]
    fn cfg_test_mod_is_skipped() {
        let src = "#[cfg(test)] mod tests { fn helper() { if true {} } } fn keep() {}";
        let fns = snippet(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }

    #[test]
    fn impl_methods_are_prefixed() {
        let src = "struct Foo; impl Foo { fn bar(&self) { if true {} } }";
        let fns = snippet(src);
        assert_eq!(fns[0].name, "Foo::bar");
        assert_eq!(fns[0].cyclomatic, 2);
    }

    #[test]
    fn loop_adds_one() {
        let fns = snippet("fn f() { loop { break; } }");
        assert_eq!(fns[0].cyclomatic, 2);
    }

    #[test]
    fn bitwise_and_does_not_add() {
        let fns = snippet("fn f(a: i32, b: i32) { let _ = a & b; }");
        assert_eq!(fns[0].cyclomatic, 1);
    }

    #[test]
    fn closure_decisions_fold_into_enclosing_fn() {
        let fns = snippet("fn f() { let _ = || { if true {} }; }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].cyclomatic, 2);
    }

    #[test]
    fn nested_fn_is_scored_separately() {
        let fns = snippet("fn outer() { fn inner() { if true {} } }");
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "outer");
        assert_eq!(fns[0].cyclomatic, 1);
        assert_eq!(fns[1].name, "inner");
        assert_eq!(fns[1].cyclomatic, 2);
    }

    #[test]
    fn trait_default_is_scored() {
        let fns = snippet("trait T { fn m(&self) { if true {} } }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "T::m");
        assert_eq!(fns[0].cyclomatic, 2);
    }

    #[test]
    fn parseable_macro_tokens_add_decisions() {
        let fns = snippet("fn f() { m!(if true {}); }");
        assert_eq!(fns[0].cyclomatic, 2);
    }

    #[test]
    fn opaque_macro_tokens_do_not_add() {
        let fns = snippet("fn f() { opaque!(@@@); }");
        assert_eq!(fns[0].cyclomatic, 1);
    }
}
