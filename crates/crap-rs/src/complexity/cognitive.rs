//! Cognitive complexity: nesting-weighted control flow.

use super::{ParsedMacro, visit_parsed_macro};
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
                if matches!(&arm.pat, syn::Pat::Guard(_)) {
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
#[path = "cognitive_tests.rs"]
mod tests;
