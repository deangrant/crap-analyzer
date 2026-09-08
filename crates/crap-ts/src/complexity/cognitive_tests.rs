use super::count;

#[test]
fn nested_if_adds_nesting() {
    assert_eq!(count("{ if (x) { if (y) { z; } } }"), 3);
}

#[test]
fn else_is_flat() {
    assert_eq!(count("{ if (x) { a; } else { b; } }"), 2);
}

#[test]
fn bool_run_counts_once() {
    assert_eq!(count("{ if (a && b && c) { x; } }"), 2);
}

#[test]
fn or_and_nullish_runs() {
    assert_eq!(count("{ if (a || b || c) { x; } }"), 2);
    assert_eq!(count("{ if (a ?? b ?? c) { x; } }"), 2);
}

#[test]
fn while_for_do_switch_catch() {
    let body = "{ while (x) { } for (;;) { } do { } while (0); switch (x) { } catch { } }";
    assert!(count(body) >= 4);
}

#[test]
fn control_without_brace() {
    assert_eq!(count("{ if (x); }"), 1);
}

#[test]
fn unclosed_brace_after_control() {
    assert_eq!(count("{ if (x) { y "), 1);
}

#[test]
fn trailing_comment_eof() {
    assert_eq!(count("{ x; // trailing"), 0);
}

#[test]
fn top_level_bool() {
    assert_eq!(count("{ a && b }"), 1);
}

#[test]
fn find_block_open_eof() {
    assert_eq!(count("{ if x //"), 1);
}

#[test]
fn scan_bools_stops() {
    let body = "{ if (f(a && b, c || d)) { x; } }";
    assert!(count(body) >= 2);
}

#[test]
fn scan_bools_non_op_chars() {
    assert_eq!(count("{ if (x) { y; } }"), 1);
}

#[test]
fn find_block_open_walks_to_eof() {
    assert_eq!(count("{ if x }"), 1);
}

#[test]
fn bool_run_to_eof() {
    assert_eq!(count("{ a &&"), 1);
}

#[test]
fn scan_bools_noise_to_eof() {
    assert_eq!(count("{ if/**/{ x; } }"), 1);
}

#[test]
fn bool_run_comment_to_eof() {
    assert_eq!(count("{ a && /*"), 1);
}
