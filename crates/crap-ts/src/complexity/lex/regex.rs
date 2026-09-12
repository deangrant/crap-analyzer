//! Regex literal scanning for TypeScript noise skip.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::{ident_start_before, is_ident_byte, prev_code_byte, prev_non_ws};

pub(super) fn can_start_regex(bytes: &[u8], i: usize) -> bool {
    let Some(prev) = prev_code_byte(bytes, i) else {
        return true;
    };
    if is_regex_prev_punct(prev) {
        return true;
    }
    is_ident_byte(prev) && precedes_regex_keyword(bytes, i)
}

const fn is_regex_prev_punct(b: u8) -> bool {
    matches!(
        b,
        b'(' | b'['
            | b'{'
            | b'='
            | b':'
            | b','
            | b';'
            | b'!'
            | b'&'
            | b'|'
            | b'?'
            | b'+'
            | b'-'
            | b'*'
            | b'%'
            | b'~'
            | b'^'
            | b'\n'
            | b'\r'
    )
}

fn precedes_regex_keyword(bytes: &[u8], i: usize) -> bool {
    let end = prev_non_ws(bytes, i);
    let start = ident_start_before(bytes, end);
    REGEX_KEYWORDS.contains(&&bytes[start..end])
}

const REGEX_KEYWORDS: &[&[u8]] = &[
    b"return",
    b"throw",
    b"case",
    b"else",
    b"typeof",
    b"void",
    b"delete",
    b"new",
    b"await",
    b"yield",
    b"in",
    b"of",
    b"instanceof",
    b"do",
];

pub(super) fn skip_regex(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut in_class = false;
    while i < bytes.len() {
        match step_regex(bytes, i, &mut in_class) {
            RegexStep::Continue(next) => i = next,
            RegexStep::Done(end) => return Some(end),
            RegexStep::Fail => return None,
        }
    }
    None
}

enum RegexStep {
    Continue(usize),
    Done(usize),
    Fail,
}

fn step_regex(bytes: &[u8], i: usize, in_class: &mut bool) -> RegexStep {
    if let Some(step) = class_or_escape(bytes, i, in_class) {
        return step;
    }
    end_or_continue(bytes, i, *in_class)
}

fn class_or_escape(bytes: &[u8], i: usize, in_class: &mut bool) -> Option<RegexStep> {
    let b = bytes[i];
    if b == b'\\' {
        return Some(RegexStep::Continue(i.saturating_add(2)));
    }
    if b == b'[' {
        *in_class = true;
        return Some(RegexStep::Continue(i + 1));
    }
    if b == b']' {
        *in_class = false;
        return Some(RegexStep::Continue(i + 1));
    }
    None
}

fn end_or_continue(bytes: &[u8], i: usize, in_class: bool) -> RegexStep {
    let b = bytes[i];
    if b == b'\n' {
        return RegexStep::Fail;
    }
    if is_regex_close(b, in_class) {
        return RegexStep::Done(after_regex_flags(bytes, i + 1));
    }
    RegexStep::Continue(i + 1)
}

const fn is_regex_close(b: u8, in_class: bool) -> bool {
    b == b'/' && !in_class
}

fn after_regex_flags(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b'g' | b'i' | b'm' | b's' | b'u' | b'y' | b'd') {
        i += 1;
    }
    i
}
