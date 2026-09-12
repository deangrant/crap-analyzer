// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::validate_structure;

#[test]
fn valid_source_passes() {
    let src = b"function f() { return \"{}\"; }\n";
    assert!(validate_structure(src).is_ok());
}

#[test]
fn braces_inside_string_and_comment_ok() {
    let src = b"/* { */\nfunction f() { const s = \"{\"; }\n";
    assert!(validate_structure(src).is_ok());
}

#[test]
fn unclosed_string_fails() {
    let src = b"function f() { const s = \"oops; }\n";
    assert_eq!(validate_structure(src), Err("unclosed string"));
}

#[test]
fn unclosed_template_fails() {
    let src = b"function f() { const s = `oops; }\n";
    assert_eq!(validate_structure(src), Err("unclosed string"));
}

#[test]
fn unclosed_comment_fails() {
    let src = b"/* still open\nfunction f() {}\n";
    assert_eq!(validate_structure(src), Err("unclosed comment"));
}

#[test]
fn unbalanced_braces_fail() {
    let src = b"function f() {\n";
    assert_eq!(validate_structure(src), Err("unbalanced braces"));
}

#[test]
fn mismatched_closer_fails() {
    let src = b"function f() { )\n";
    assert_eq!(validate_structure(src), Err("unbalanced braces"));
}

#[test]
fn extra_closer_fails() {
    let src = b"}\n";
    assert_eq!(validate_structure(src), Err("unbalanced braces"));
}

#[test]
fn template_with_expression_ok() {
    let src = b"function f(x: number) { return `${x}`; }\n";
    assert!(validate_structure(src).is_ok());
}

#[test]
fn nonsense_tokens_with_balanced_braces_pass() {
    let src = b"function f() { notValidTypeScript $$$ { nested } }\n";
    assert!(validate_structure(src).is_ok());
}
