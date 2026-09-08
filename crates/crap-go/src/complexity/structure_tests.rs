use super::validate_structure;

#[test]
fn valid_source_passes() {
    let src = b"package p\nfunc F() { return \"{}\" }\n";
    assert!(validate_structure(src).is_ok());
}

#[test]
fn braces_inside_string_and_comment_ok() {
    let src = b"package p\n/* { */\nfunc F() { s := \"{\" }\n";
    assert!(validate_structure(src).is_ok());
}

#[test]
fn unclosed_string_fails() {
    let src = b"package p\nfunc F() { s := \"oops }\n";
    assert_eq!(validate_structure(src), Err("unclosed string"));
}

#[test]
fn unclosed_raw_string_fails() {
    let src = b"package p\nfunc F() { s := `oops }\n";
    assert_eq!(validate_structure(src), Err("unclosed string"));
}

#[test]
fn unclosed_comment_fails() {
    let src = b"package p\n/* still open\nfunc F() {}\n";
    assert_eq!(validate_structure(src), Err("unclosed comment"));
}

#[test]
fn unbalanced_braces_fail() {
    let src = b"package p\nfunc F() {\n";
    assert_eq!(validate_structure(src), Err("unbalanced braces"));
}

#[test]
fn mismatched_closer_fails() {
    let src = b"package p\nfunc F() { )\n";
    assert_eq!(validate_structure(src), Err("unbalanced braces"));
}

#[test]
fn extra_closer_fails() {
    let src = b"package p\n}\n";
    assert_eq!(validate_structure(src), Err("unbalanced braces"));
}

#[test]
fn nonsense_tokens_with_balanced_braces_pass() {
    let src = b"package p\nfunc F() { notReal $$$ syntax }\n";
    assert!(validate_structure(src).is_ok());
}
