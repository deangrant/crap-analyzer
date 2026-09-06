//! Cognitive complexity: nesting-weighted control flow.

use super::parse_macro_body;
use syn::visit::{self, Visit};
use syn::{BinOp, Expr, ExprBinary};

/// Returns cognitive complexity for `body` (minimum 0).
pub(super) fn count(body: &syn::Block) -> usize {
    let mut counter = CognitiveCounter {
        count: 0,
        nesting: 0,
    };
    counter.visit_block(body);
    counter.count
}

struct CognitiveCounter {
    count: usize,
    nesting: usize,
}

impl CognitiveCounter {
    const fn add_nested(&mut self) {
        self.count += 1 + self.nesting;
    }

    const fn enter(&mut self) {
        self.nesting += 1;
    }

    const fn leave(&mut self) {
        self.nesting = self.nesting.saturating_sub(1);
    }

    fn score_if(&mut self, node: &syn::ExprIf, else_if: bool) {
        if else_if {
            self.count += 1;
        } else {
            self.add_nested();
        }
        self.visit_expr(&node.cond);
        self.enter();
        self.visit_block(&node.then_branch);
        self.leave();
        self.score_else(node);
    }

    fn score_else(&mut self, node: &syn::ExprIf) {
        let Some((_, else_expr)) = &node.else_branch else {
            return;
        };
        if let Expr::If(inner) = else_expr.as_ref() {
            self.score_if(inner, true);
            return;
        }
        self.count += 1;
        self.enter();
        self.visit_expr(else_expr);
        self.leave();
    }

    fn score_loop_like(&mut self, walk: impl FnOnce(&mut Self)) {
        self.add_nested();
        self.enter();
        walk(self);
        self.leave();
    }
}

impl<'ast> Visit<'ast> for CognitiveCounter {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.score_if(node, false);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.score_loop_like(|this| visit::visit_expr_for_loop(this, node));
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.score_loop_like(|this| visit::visit_expr_while(this, node));
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.score_loop_like(|this| visit::visit_expr_loop(this, node));
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.visit_expr(&node.expr);
        self.score_loop_like(|this| {
            for arm in &node.arms {
                if arm.guard.is_some() {
                    this.count += 1;
                }
                visit::visit_arm(this, arm);
            }
        });
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        self.visit_pat(&node.pat);
        let Some(init) = &node.init else {
            return;
        };
        self.visit_expr(&init.expr);
        let Some((_, diverge)) = &init.diverge else {
            return;
        };
        self.add_nested();
        self.count += 1;
        self.enter();
        self.visit_expr(diverge);
        self.leave();
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.enter();
        visit::visit_expr_closure(self, node);
        self.leave();
    }

    fn visit_expr_binary(&mut self, node: &'ast ExprBinary) {
        if logical_kind(node.op).is_some() {
            score_bool_chain(self, node);
            return;
        }
        visit::visit_expr_binary(self, node);
    }

    fn visit_expr_break(&mut self, node: &'ast syn::ExprBreak) {
        if node.label.is_some() {
            self.count += 1;
        }
        visit::visit_expr_break(self, node);
    }

    fn visit_expr_continue(&mut self, node: &'ast syn::ExprContinue) {
        if node.label.is_some() {
            self.count += 1;
        }
        visit::visit_expr_continue(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.visit_macro_tokens(&node.tokens);
    }

    fn visit_item(&mut self, _node: &'ast syn::Item) {}
}

impl CognitiveCounter {
    fn visit_macro_tokens(&mut self, tokens: &proc_macro2::TokenStream) {
        let Some((expr, stmts)) = parse_macro_body(tokens) else {
            return;
        };
        if let Some(expr) = &expr {
            self.visit_expr(expr);
            return;
        }
        for stmt in &stmts {
            self.visit_stmt(stmt);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Logical {
    And,
    Or,
}

const fn logical_kind(op: BinOp) -> Option<Logical> {
    match op {
        BinOp::And(_) => Some(Logical::And),
        BinOp::Or(_) => Some(Logical::Or),
        _ => None,
    }
}

fn score_bool_chain(counter: &mut CognitiveCounter, node: &ExprBinary) {
    let Some(kind) = logical_kind(node.op) else {
        return;
    };
    counter.count += 1;
    walk_bool_side(counter, &node.left, kind);
    walk_bool_side(counter, &node.right, kind);
}

fn walk_bool_side(counter: &mut CognitiveCounter, expr: &Expr, parent: Logical) {
    let Expr::Binary(bin) = expr else {
        counter.visit_expr(expr);
        return;
    };
    let Some(kind) = logical_kind(bin.op) else {
        counter.visit_expr(expr);
        return;
    };
    if kind != parent {
        counter.count += 1;
    }
    walk_bool_side(counter, &bin.left, kind);
    walk_bool_side(counter, &bin.right, kind);
}

#[cfg(test)]
mod tests {
    use super::super::analyze_source;
    use crap_core::Metric;
    use std::path::Path;

    fn snippet(src: &str) -> usize {
        let parsed = analyze_source(Path::new("t.rs"), src, Metric::Cognitive);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default().first().map_or(0, |f| f.complexity)
    }

    #[test]
    fn straight_line_is_zero() {
        assert_eq!(snippet("fn trivial() { let x = 1; }"), 0);
    }

    #[test]
    fn uninitialized_let_does_not_add() {
        assert_eq!(snippet("fn f() { let x; }"), 0);
    }

    #[test]
    fn top_level_if_is_one() {
        assert_eq!(snippet("fn f(x: i32) { if x > 0 { x; } }"), 1);
    }

    #[test]
    fn nested_if_adds_nesting_penalty() {
        let src = "fn f(x: i32) { if x > 0 { if x > 1 { x; } } }";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn else_if_is_flat() {
        let src = "fn f(x: i32) { if x > 0 { x; } else if x < 0 { x; } else { 0; } }";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn same_bool_sequence_counts_once() {
        assert_eq!(
            snippet("fn f(a: bool, b: bool, c: bool) { let _ = a && b && c; }"),
            1
        );
    }

    #[test]
    fn bool_operator_switch_adds_one() {
        assert_eq!(
            snippet("fn f(a: bool, b: bool, c: bool) { let _ = a && b || c; }"),
            2
        );
    }

    #[test]
    fn closure_raises_nesting() {
        let src = "fn f() { let _ = || { if true {} }; }";
        assert_eq!(snippet(src), 2);
    }

    #[test]
    fn labeled_break_adds_one() {
        assert_eq!(snippet("fn f() { 'a: loop { break 'a; } }"), 2);
    }

    #[test]
    fn unlabeled_break_is_free() {
        assert_eq!(snippet("fn f() { loop { break; } }"), 1);
    }

    #[test]
    fn question_mark_does_not_add() {
        assert_eq!(snippet("fn f() -> Result<(), ()> { Ok(())?; Ok(()) }"), 0);
    }

    #[test]
    fn match_is_one_increment() {
        let src = "fn f(x: i32) { match x { 0 => {}, 1 => {}, _ => {} } }";
        assert_eq!(snippet(src), 1);
    }

    #[test]
    fn one_empty_match_arm_is_still_one() {
        assert_eq!(snippet("fn f(x: i32) { match x { _ => {} } }"), 1);
    }

    #[test]
    fn nested_match_adds_nesting_penalty() {
        let src = "fn f(x: i32) { if x > 0 { match x { 0 => {}, 1 => {}, _ => {} } } }";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn let_else_scores_like_if_else() {
        assert_eq!(
            snippet("fn f(r: Result<i32, ()>) { let Ok(x) = r else { return; }; x; }"),
            2
        );
    }

    #[test]
    fn let_else_matches_if_let_else() {
        let let_else = snippet("fn f(r: Option<i32>) { let Some(x) = r else { 0; }; x; }");
        let if_let = snippet("fn f(r: Option<i32>) { if let Some(x) = r { x; } else { 0; } }");
        assert_eq!(let_else, 2);
        assert_eq!(if_let, 2);
    }

    #[test]
    fn nested_if_inside_let_else_pays_nesting() {
        let src = "fn f(r: Result<i32, ()>) { let Ok(x) = r else { if true { return; } }; x; }";
        assert_eq!(snippet(src), 4);
    }

    #[test]
    fn match_guard_adds_one() {
        let src = "fn f(n: i32) { match n { n if n > 0 => {}, _ => {} } }";
        assert_eq!(snippet(src), 2);
    }

    #[test]
    fn match_guard_bool_chain_still_adds() {
        let src = "fn f(n: i32, a: bool) { match n { n if n > 0 && a => {}, _ => {} } }";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn match_guard_range_bool_chain_adds() {
        let src = "fn f(n: i32) { match n { n if n > 0 && n < 10 => {}, _ => {} } }";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn two_match_guards_add_two() {
        let src = "fn f(n: i32) { match n { n if n > 0 => {}, n if n < 0 => {}, _ => {} } }";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn wide_flat_match_is_still_one() {
        let src = "fn f(x: i32) {
            match x {
                0 => {}, 1 => {}, 2 => {}, 3 => {}, 4 => {},
                5 => {}, 6 => {}, 7 => {}, 8 => {}, 9 => {},
                10 => {}, 11 => {}, 12 => {}, 13 => {}, 14 => {},
                15 => {}, 16 => {}, 17 => {}, 18 => {}, _ => {},
            }
        }";
        assert_eq!(snippet(src), 1);
    }

    #[test]
    fn nested_fn_is_scored_separately() {
        let parsed = analyze_source(
            Path::new("t.rs"),
            "fn outer() { fn inner() { if true {} } }",
            Metric::Cognitive,
        );
        assert!(parsed.is_ok());
        let fns = parsed.unwrap_or_default();
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].complexity, 0);
        assert_eq!(fns[1].complexity, 1);
    }

    #[test]
    fn parseable_macro_tokens_add_decisions() {
        assert_eq!(snippet("fn f() { m!(if true {}); }"), 1);
    }

    #[test]
    fn statement_macro_tokens_add_decisions() {
        assert_eq!(snippet("fn f() { m!(let x = 1; if true { x; }); }"), 1);
    }

    #[test]
    fn opaque_macro_tokens_do_not_add() {
        assert_eq!(snippet("fn f() { opaque!(@@@); }"), 0);
    }

    #[test]
    fn labeled_continue_adds_one() {
        assert_eq!(snippet("fn f() { 'a: loop { continue 'a; } }"), 2);
    }

    #[test]
    fn for_loop_adds_one() {
        assert_eq!(snippet("fn f(xs: &[i32]) { for _ in xs {} }"), 1);
    }

    #[test]
    fn while_loop_adds_one() {
        assert_eq!(snippet("fn f(mut n: i32) { while n > 0 { n -= 1; } }"), 1);
    }

    #[test]
    fn non_logical_binary_inside_and_is_walked() {
        assert_eq!(
            snippet("fn f(a: bool, x: i32, y: i32) { let _ = a && x + y > 0; }"),
            1
        );
    }

    #[test]
    fn async_wrapper_does_not_add() {
        assert_eq!(snippet("async fn f() { if true {} }"), 1);
        assert_eq!(snippet("fn f() { async { if true {} }; }"), 1);
    }

    #[test]
    fn try_block_does_not_add() {
        assert_eq!(snippet("fn f() { try { if true {} } }"), 1);
    }

    #[test]
    fn score_bool_chain_ignores_non_logical_ops() {
        let parsed = syn::parse_str::<syn::Expr>("a + b");
        assert!(parsed.is_ok());
        let Ok(super::Expr::Binary(bin)) = parsed else {
            return;
        };
        let mut counter = super::CognitiveCounter {
            count: 0,
            nesting: 0,
        };
        super::score_bool_chain(&mut counter, &bin);
        assert_eq!(counter.count, 0);
    }
}
