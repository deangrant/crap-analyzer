use super::count;

#[test]
fn nested_if_is_nesting_weighted() {
    let body = "{ if a { if b { x } } }";
    // outer if: 1+0=1, inner if: 1+1=2 → total 3
    assert_eq!(count(body), 3);
}

#[test]
fn else_is_flat_one() {
    let body = "{ if a { x } else { y } }";
    // if: 1, else: 1 → 2
    assert_eq!(count(body), 2);
}

#[test]
fn else_if_is_flat_else_plus_if() {
    let body = "{ if a { x } else if b { y } }";
    // if: 1, else: 1, if: 1 → 3
    assert_eq!(count(body), 3);
}

#[test]
fn bool_run_counts_once() {
    let body = "{ if a && b && c { x } }";
    // if: 1, && run: 1 → 2
    assert_eq!(count(body), 2);
}

#[test]
fn or_run_counts_once() {
    let body = "{ if a || b || c { x } }";
    assert_eq!(count(body), 2);
}

#[test]
fn comments_and_strings_are_skipped() {
    let body = r#"{
// if fake {
/* for also */
x := "if" + `for` + 's'
if a { x }
}"#;
    assert_eq!(count(body), 1);
}

#[test]
fn escaped_quotes_inside_strings() {
    let body = r#"{ x := "a\"b"; if y { z } }"#;
    assert_eq!(count(body), 1);
}

#[test]
fn trailing_comment_reaches_eof_after_noise() {
    assert_eq!(count("{ x := 1 // trailing"), 0);
}

#[test]
fn top_level_bool_run() {
    assert_eq!(count("{ a && b }"), 1);
}

#[test]
fn control_without_brace_stops_at_semicolon() {
    assert_eq!(count("{ if x; y }"), 1);
}

#[test]
fn unclosed_brace_after_control() {
    assert_eq!(count("{ if x { y "), 1);
}

#[test]
fn bool_scan_stops_on_comma_and_paren() {
    let body = "{ if f(a && b, c || d) { x } }";
    assert!(count(body) >= 2);
}

#[test]
fn switch_and_select_are_nested() {
    let body = "{ switch x { default: } select { default: } }";
    assert_eq!(count(body), 2);
}

#[test]
fn unmatched_open_brace_helpers() {
    assert_eq!(count("{ if"), 1);
}

#[test]
fn scan_bools_comment_only_header() {
    assert_eq!(count("{ if //\n{ x } }"), 1);
    // Block comment consumes the whole header so skip_noise lands at EOF.
    assert_eq!(count("{ if/**/{ x } }"), 1);
}

#[test]
fn find_block_open_eof_after_noise() {
    assert_eq!(count("{ if x //"), 1);
}

#[test]
fn match_braces_edges() {
    assert!(super::super::lex::skip_balanced(b"x", 0, b'{', b'}').is_none());
    assert!(super::super::lex::skip_balanced(b"{ /*", 0, b'{', b'}').is_none());
}

#[test]
fn unterminated_string_returns_len() {
    assert_eq!(super::super::lex::skip_string(br#""abc"#, 0), 4);
    assert_eq!(super::super::lex::skip_string(b"", 0), 0);
}

#[test]
fn skip_to_bool_eof_after_comment() {
    // `&&` run whose right-hand side is only a comment.
    assert_eq!(count("{ a && //"), 1);
}
