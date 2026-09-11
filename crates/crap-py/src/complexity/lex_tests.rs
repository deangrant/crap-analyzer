use super::*;

#[test]
fn skip_noise_skips_hash_comment() {
    let src = b"# comment\nx";
    // Line comments stop at the newline; the newline itself is not noise.
    assert_eq!(skip_noise(src, 0), 9);
}

#[test]
fn skip_string_handles_quotes_and_prefix() {
    assert_eq!(skip_string(br#""hi""#, 0), 4);
    assert_eq!(skip_string(br"r'hi'", 0), 5);
    assert_eq!(skip_string(b"'''hi'''", 0), 8);
    assert_eq!(skip_string(br#"rf"hi""#, 0), 6);
    assert_eq!(skip_string(br"fr'hi'", 0), 6);
}

#[test]
fn skip_string_handles_escapes_and_unclosed() {
    assert_eq!(skip_string(br#""a\"b""#, 0), 6);
    assert_eq!(skip_string(b"\"oops", 0), 5);
    assert_eq!(skip_string(b"'''oops", 0), 7);
    assert_eq!(skip_string(b"\"a\nb\"", 0), 5);
    assert_eq!(skip_string(b"'''a\\'''b'''", 0), 12);
}

#[test]
fn is_word_requires_boundaries() {
    assert!(is_word(b"if x", 0, b"if"));
    assert!(!is_word(b"iffy", 0, b"if"));
}

#[test]
fn try_skip_unclosed_string_errors() {
    let outcome = try_skip_noise_token(b"\"oops", 0);
    assert!(matches!(outcome, Some(Err("unclosed string"))));
}

#[test]
fn skip_balanced_parens() {
    assert_eq!(skip_balanced(b"(a(b))", 0, b'(', b')'), Some(6));
    assert_eq!(skip_balanced(b"(a", 0, b'(', b')'), None);
    assert_eq!(skip_balanced(b"x", 0, b'(', b')'), None);
    assert_eq!(skip_balanced(b"(#", 0, b'(', b')'), None);
}

#[test]
fn skip_string_prefix_without_quote_reaches_eof() {
    assert_eq!(skip_string(b"rx", 0), 2);
}

#[test]
fn string_prefix_without_quote_is_not_noise() {
    assert_eq!(skip_noise(b"raw", 0), 0);
}

#[test]
fn try_skip_noise_none_on_code() {
    assert!(try_skip_noise_token(b"def", 0).is_none());
}
