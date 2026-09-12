// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::{TAB_WIDTH, leading_indent, next_tab_stop, validate_indents};

#[test]
fn spaces_count_one_each() {
    assert_eq!(leading_indent(b"    x", 0, 5), Ok((4, 4)));
}

#[test]
fn tab_expands_to_tab_width() {
    assert_eq!(leading_indent(b"\tx", 0, 2), Ok((TAB_WIDTH, 1)));
    assert_eq!(leading_indent(b"\t\tx", 0, 3), Ok((TAB_WIDTH * 2, 2)));
}

#[test]
fn next_tab_stop_advances_to_multiple() {
    assert_eq!(next_tab_stop(0), 8);
    assert_eq!(next_tab_stop(8), 16);
}

#[test]
fn mixed_leading_whitespace_is_error() {
    assert!(leading_indent(b" \tx", 0, 3).is_err());
    assert!(leading_indent(b"\t x", 0, 3).is_err());
    assert!(validate_indents(b"def f():\n \tpass\n").is_err());
}
