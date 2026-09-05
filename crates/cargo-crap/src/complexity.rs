//! Cyclomatic complexity and source spans for Rust functions.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{BinOp, ImplItemFn, ItemFn, ItemImpl};

/// One function's complexity and inclusive line span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionComplexity {
    /// Source path as supplied to the walker.
    pub file: PathBuf,
    /// Free function name, or `Type::method` for impl methods.
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
    };
    visitor.visit_file(&syntax);
    Ok(visitor.out)
}

fn has_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg") && attr.parse_args::<syn::Ident>().is_ok_and(|id| id == "test")
    })
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
        if has_attr(&node.attrs, "test") {
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
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev = self.impl_type.take();
        self.impl_type = impl_type_name(&node.self_ty);
        visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if has_attr(&node.attrs, "test") {
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

    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}

    fn visit_item(&mut self, _node: &'ast syn::Item) {}
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
}
