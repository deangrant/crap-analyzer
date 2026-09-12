// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::{
    is_ident_byte, is_ident_start, is_word, match_op, skip_balanced, skip_noise, skip_string,
};

#[test]
fn lone_slash_is_not_noise() {
    assert_eq!(skip_noise(b"/", 0), 0);
    assert_eq!(skip_noise(b"/x", 0), 0);
}

#[test]
fn skips_line_and_block_comments() {
    assert_eq!(skip_noise(b"// hi\nx", 0), 5);
    assert_eq!(skip_noise(b"/* hi */x", 0), 8);
}

#[test]
fn skips_template_with_interpolation() {
    assert_eq!(skip_noise(b"`a${x}b`y", 0), 8);
}

#[test]
fn skips_quoted_strings_and_escapes() {
    assert_eq!(skip_string(b"\"a\\\"b\"x", 0), 6);
    assert_eq!(skip_string(b"'\\''x", 0), 4);
}

#[test]
fn unterminated_string_reaches_eof() {
    assert_eq!(skip_string(b"\"abc", 0), 4);
    assert_eq!(skip_string(b"`abc", 0), 4);
}

#[test]
fn regex_after_punct_and_return() {
    assert_eq!(skip_noise(b"( /a/ )", 2), 5);
    assert_eq!(skip_noise(b"return /if/;", 7), 11);
}

#[test]
fn regex_with_class_and_flags() {
    assert_eq!(skip_noise(b"=/[a\\/]+/gi ", 1), 11);
}

#[test]
fn regex_unterminated_is_not_noise() {
    assert_eq!(skip_noise(b"=/a", 1), 1);
}

#[test]
fn balanced_braces_and_words() {
    assert_eq!(skip_balanced(b"{ a }", 0, b'{', b'}'), Some(5));
    assert!(skip_balanced(b"{ a", 0, b'{', b'}').is_none());
    assert!(skip_balanced(b"a", 0, b'{', b'}').is_none());
    assert!(!match_op(b"&", 0, b"&&"));
    assert!(is_word(b" if ", 1, b"if"));
    assert!(is_ident_start(b'a') && is_ident_byte(b'$'));
}

#[test]
fn jsx_tag_and_generics_context() {
    assert_eq!(skip_noise(b"return <div/>;", 7), 13);
    assert_eq!(skip_noise(b"Foo<Bar>", 3), 3);
    assert_eq!(skip_noise(b"return </div>;", 7), 13);
    assert_eq!(skip_noise(b"return <>;", 7), 9);
}

#[test]
fn jsx_with_quoted_attrs_and_expr() {
    let src = br#"return <div id="a\"b" x={1}>;"#;
    let end = skip_noise(src, 7);
    assert!(end > 7);
}

#[test]
fn template_escape_and_unclosed_interp() {
    assert_eq!(skip_noise(b"`a\\`b`x", 0), 6);
    assert_eq!(skip_noise(b"`a${", 0), 4);
}

#[test]
fn skip_noise_empty() {
    assert_eq!(skip_noise(b"", 0), 0);
}

#[test]
fn skip_string_empty_start() {
    assert_eq!(skip_string(b"", 0), 0);
}

#[test]
fn regex_keywords_and_newline_fail() {
    for kw in [
        "throw",
        "case",
        "new",
        "in",
        "instanceof",
        "typeof",
        "void",
        "delete",
        "await",
        "yield",
        "of",
        "do",
        "else",
    ] {
        let src = format!("{kw} /a/;");
        let start = kw.len() + 1;
        assert!(skip_noise(src.as_bytes(), start) > start, "keyword {kw}");
    }
    assert_eq!(skip_noise(b"=/\nx", 1), 1);
}

#[test]
fn jsx_edge_cases() {
    assert_eq!(skip_noise(b"<", 0), 0);
    assert_eq!(skip_noise(b"<div>", 0), 5);
    assert_eq!(skip_noise(b"( <div> )", 2), 7);
    assert_eq!(skip_noise(b"x < y", 2), 2);
    // Unclosed JSX expression attribute fails the tag skip → not noise.
    assert_eq!(skip_noise(b"return <div x={;", 7), 7);
    // Unterminated tag reaches EOF without `>`.
    assert_eq!(skip_noise(b"return <div", 7), 7);
    assert_eq!(skip_noise(b"/ <", 0), 0);
}

#[test]
fn regex_at_file_start_and_ws_keyword() {
    assert!(skip_noise(b"/a/", 0) >= 3);
    assert_eq!(skip_noise(b"return\n/a/;", 7), 10);
}

#[test]
fn balanced_unclosed_returns_none() {
    assert!(skip_balanced(b"{ a", 0, b'{', b'}').is_none());
    assert!(skip_balanced(b"", 0, b'{', b'}').is_none());
    assert!(skip_balanced(b"{ /*", 0, b'{', b'}').is_none());
}

#[test]
fn prev_non_ws_all_spaces() {
    assert_eq!(super::prev_non_ws(b"   /", 3), 0);
}
