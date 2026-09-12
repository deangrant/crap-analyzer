//! JSX tag scanning for TypeScript noise skip.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::{
    ident_start_before, is_ident_byte, is_ident_start, prev_code_byte, prev_non_ws, skip_balanced,
};

pub(super) fn looks_like_jsx_tag(bytes: &[u8], i: usize) -> bool {
    bytes.get(i) == Some(&b'<') && can_start_jsx(bytes, i) && jsx_tag_follows(bytes, i)
}

fn jsx_tag_follows(bytes: &[u8], i: usize) -> bool {
    let Some(&b) = bytes.get(i + 1) else {
        return false;
    };
    jsx_second_byte(bytes, b, i)
}

fn jsx_second_byte(bytes: &[u8], b: u8, i: usize) -> bool {
    if b == b'/' {
        return closing_jsx_name(bytes, i);
    }
    b == b'>' || is_ident_start(b)
}

fn closing_jsx_name(bytes: &[u8], i: usize) -> bool {
    bytes.get(i + 2).is_some_and(|&c| is_ident_start(c))
}

fn can_start_jsx(bytes: &[u8], i: usize) -> bool {
    let Some(prev) = prev_code_byte(bytes, i) else {
        return true;
    };
    if is_jsx_prev_punct(prev) {
        return true;
    }
    is_ident_byte(prev) && precedes_jsx_keyword(bytes, i)
}

const fn is_jsx_prev_punct(b: u8) -> bool {
    matches!(
        b,
        b'(' | b'{'
            | b'['
            | b'='
            | b':'
            | b','
            | b';'
            | b'!'
            | b'?'
            | b'&'
            | b'|'
            | b'>'
            | b'\n'
            | b'\r'
    )
}

fn precedes_jsx_keyword(bytes: &[u8], i: usize) -> bool {
    let end = prev_non_ws(bytes, i);
    let start = ident_start_before(bytes, end);
    JSX_KEYWORDS.contains(&&bytes[start..end])
}

const JSX_KEYWORDS: &[&[u8]] = &[
    b"return", b"throw", b"else", b"typeof", b"await", b"yield", b"case", b"default", b"do", b"of",
    b"in", b"void", b"new", b"delete",
];

pub(super) fn skip_jsx_tag(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut in_quote: Option<u8> = None;
    while i < bytes.len() {
        match step_jsx(bytes, i, &mut in_quote) {
            JsxOut::Done(end) => return Some(end),
            JsxOut::Next(next) => i = next,
            JsxOut::Fail => return None,
        }
    }
    None
}

enum JsxOut {
    Done(usize),
    Next(usize),
    Fail,
}

fn step_jsx(bytes: &[u8], i: usize, in_quote: &mut Option<u8>) -> JsxOut {
    if let Some(q) = *in_quote {
        return JsxOut::Next(after_jsx_quote_byte(bytes, i, q, in_quote));
    }
    unquoted_jsx_byte(bytes, i, in_quote)
}

fn unquoted_jsx_byte(bytes: &[u8], i: usize, in_quote: &mut Option<u8>) -> JsxOut {
    let b = bytes[i];
    if is_jsx_attr_quote(b) {
        *in_quote = Some(b);
        return JsxOut::Next(i + 1);
    }
    if b == b'{' {
        return balanced_jsx_or_fail(bytes, i);
    }
    if b == b'>' {
        return JsxOut::Done(i + 1);
    }
    JsxOut::Next(i + 1)
}

const fn is_jsx_attr_quote(b: u8) -> bool {
    b == b'"' || b == b'\''
}

fn balanced_jsx_or_fail(bytes: &[u8], i: usize) -> JsxOut {
    skip_balanced(bytes, i, b'{', b'}').map_or(JsxOut::Fail, JsxOut::Next)
}

fn after_jsx_quote_byte(bytes: &[u8], i: usize, quote: u8, in_quote: &mut Option<u8>) -> usize {
    if bytes[i] == quote {
        *in_quote = None;
        return i + 1;
    }
    if bytes[i] == b'\\' {
        return i.saturating_add(2);
    }
    i + 1
}
