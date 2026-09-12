// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::validate_structure;

#[test]
fn balanced_source_ok() {
    assert!(validate_structure(b"def f(x):\n    return (x)\n").is_ok());
}

#[test]
fn unclosed_string_fails() {
    assert_eq!(validate_structure(b"x = \"oops"), Err("unclosed string"));
}

#[test]
fn unbalanced_parens_fail() {
    assert_eq!(validate_structure(b"f(a"), Err("unbalanced braces"));
}

#[test]
fn mismatched_brackets_fail() {
    assert_eq!(validate_structure(b"(]"), Err("unbalanced braces"));
}

#[test]
fn extra_close_fails() {
    assert_eq!(validate_structure(b")"), Err("unbalanced braces"));
}

#[test]
fn square_brackets_balance() {
    assert!(validate_structure(b"x = [1, 2]\n").is_ok());
    assert_eq!(validate_structure(b"x = [1, 2\n"), Err("unbalanced braces"));
}

#[test]
fn curly_braces_balance() {
    assert!(validate_structure(b"x = {}\n").is_ok());
    assert_eq!(validate_structure(b"x = {\n"), Err("unbalanced braces"));
}
