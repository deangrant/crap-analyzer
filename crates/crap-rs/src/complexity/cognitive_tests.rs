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
