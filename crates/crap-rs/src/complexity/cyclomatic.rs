//! Cyclomatic complexity: one plus each decision point.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::{ParsedMacro, visit_parsed_macro};
use syn::{
    BinOp,
    visit::{self, Visit},
};

/// Returns cyclomatic complexity for `body` (minimum 1).
pub(super) fn count(body: &syn::Block) -> usize {
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
        if matches!(&node.pat, syn::Pat::Guard(_)) {
            self.count += 1;
        }
        visit::visit_arm(self, node);
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if node.init.as_ref().is_some_and(|init| init.diverge.is_some()) {
            self.count += 1;
        }
        visit::visit_local(self, node);
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
        visit_parsed_macro(tokens, |part| self.apply_macro(part));
    }

    fn apply_macro(&mut self, part: ParsedMacro<'_>) {
        match part {
            ParsedMacro::Expr(expr) => self.visit_expr(expr),
            ParsedMacro::Stmts(stmts) => self.visit_macro_stmts(stmts),
            ParsedMacro::File(file) => self.visit_macro_items(file),
        }
    }

    fn visit_macro_stmts(&mut self, stmts: &[syn::Stmt]) {
        for stmt in stmts {
            self.visit_stmt(stmt);
        }
    }

    fn visit_macro_items(&mut self, file: &syn::File) {
        for item in &file.items {
            visit::visit_item(self, item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::analyze_source;
    use crap_core::Metric;
    use std::path::Path;

    fn snippet(src: &str) -> usize {
        let parsed = analyze_source(Path::new("t.rs"), src, Metric::Cyclomatic);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default().first().map_or(0, |f| f.complexity)
    }

    #[test]
    fn straight_line_is_one() {
        assert_eq!(snippet("fn trivial() { let x = 1; }"), 1);
    }

    #[test]
    fn if_adds_one() {
        assert_eq!(snippet("fn f(x: i32) { if x > 0 { x; } }"), 2);
    }

    #[test]
    fn and_adds_one() {
        assert_eq!(snippet("fn f(a: bool, b: bool) { let _ = a && b; }"), 2);
    }

    #[test]
    fn three_match_arms_add_three() {
        let src = "fn f(x: i32) { match x { 0 => {}, 1 => {}, _ => {} } }";
        assert_eq!(snippet(src), 4);
    }

    #[test]
    fn one_match_arm_adds_one() {
        assert_eq!(snippet("fn f(x: i32) { match x { _ => {} } }"), 2);
    }

    #[test]
    fn nested_match_arms_still_count() {
        let src = "fn f(x: i32) { if x > 0 { match x { 0 => {}, 1 => {}, _ => {} } } }";
        assert_eq!(snippet(src), 5);
    }

    #[test]
    fn loop_adds_one() {
        assert_eq!(snippet("fn f() { loop { break; } }"), 2);
    }

    #[test]
    fn bitwise_and_does_not_add() {
        assert_eq!(snippet("fn f(a: i32, b: i32) { let _ = a & b; }"), 1);
    }

    #[test]
    fn closure_decisions_fold_into_enclosing_fn() {
        assert_eq!(snippet("fn f() { let _ = || { if true {} }; }"), 2);
    }

    #[test]
    fn parseable_macro_tokens_add_decisions() {
        assert_eq!(snippet("fn f() { m!(if true {}); }"), 2);
    }

    #[test]
    fn statement_macro_tokens_add_decisions() {
        assert_eq!(snippet("fn f() { m!(let x = 1; if true { x; }); }"), 2);
    }

    #[test]
    fn item_macro_tokens_add_decisions() {
        assert_eq!(snippet("fn f() { m!(fn helper() { if true {} }); }"), 2);
    }

    #[test]
    fn opaque_macro_tokens_do_not_add() {
        assert_eq!(snippet("fn f() { opaque!(@@@); }"), 1);
    }

    #[test]
    fn for_loop_adds_one() {
        assert_eq!(snippet("fn f(xs: &[i32]) { for _ in xs {} }"), 2);
    }

    #[test]
    fn while_loop_adds_one() {
        assert_eq!(snippet("fn f(mut n: i32) { while n > 0 { n -= 1; } }"), 2);
    }

    #[test]
    fn question_mark_adds_one() {
        assert_eq!(snippet("fn f() -> Result<(), ()> { Ok(())?; Ok(()) }"), 2);
    }

    #[test]
    fn let_else_adds_one() {
        assert_eq!(
            snippet("fn f(r: Result<i32, ()>) { let Ok(x) = r else { return; }; x; }"),
            2
        );
    }

    #[test]
    fn match_guard_adds_one_on_top_of_arm() {
        let src = "fn f(n: i32) { match n { n if n > 0 => {}, _ => {} } }";
        assert_eq!(snippet(src), 4);
    }

    #[test]
    fn match_guard_and_still_adds() {
        let src = "fn f(n: i32, a: bool) { match n { n if n > 0 && a => {}, _ => {} } }";
        assert_eq!(snippet(src), 5);
    }

    #[test]
    fn match_guard_range_bool_chain_adds() {
        let src = "fn f(n: i32) { match n { n if n > 0 && n < 10 => {}, _ => {} } }";
        assert_eq!(snippet(src), 5);
    }

    #[test]
    fn let_some_else_inner_if_still_counts() {
        let src = "fn f(r: Option<i32>) { let Some(x) = r else { if true { return; } }; x; }";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn async_wrapper_does_not_add() {
        assert_eq!(snippet("async fn f() { if true {} }"), 2);
        assert_eq!(snippet("fn f() { async { if true {} }; }"), 2);
    }

    #[test]
    fn try_block_does_not_add() {
        assert_eq!(snippet("fn f() { try { if true {} } }"), 2);
    }
}
