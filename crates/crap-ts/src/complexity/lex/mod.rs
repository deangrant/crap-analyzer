//! Shared scanners for TypeScript tokens (comments, strings, braces, and words).

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
mod jsx;
mod regex;

use jsx::{looks_like_jsx_tag, skip_jsx_tag};
use regex::{can_start_regex, skip_regex};

/// Skips spaces, comments, and string / template / regex literals starting at `i`.
pub(super) fn skip_noise(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        let Some(next) = skip_noise_step(bytes, i) else {
            return i;
        };
        i = next;
    }
    i
}

fn skip_noise_step(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] == b'/' {
        return skip_slash_start(bytes, i);
    }
    if matches!(bytes[i], b'"' | b'\'' | b'`') {
        return Some(skip_string(bytes, i));
    }
    if looks_like_jsx_tag(bytes, i) {
        return skip_jsx_tag(bytes, i);
    }
    None
}

/// Tries to skip one comment, string, regex, or JSX token at `i`.
///
/// Returns `None` when `i` is not noise. `Err` means an unclosed literal.
pub(super) fn try_skip_noise_token(bytes: &[u8], i: usize) -> Option<Result<usize, &'static str>> {
    if bytes.get(i) == Some(&b'/') {
        return try_skip_slash_token(bytes, i);
    }
    if matches!(bytes.get(i), Some(&b'"' | &b'\'' | &b'`')) {
        return Some(try_skip_string(bytes, i).ok_or("unclosed string"));
    }
    if looks_like_jsx_tag(bytes, i) {
        return Some(skip_jsx_tag(bytes, i).ok_or("unclosed jsx"));
    }
    None
}

fn skip_slash_start(bytes: &[u8], i: usize) -> Option<usize> {
    let outcome = try_skip_slash_token(bytes, i)?;
    if let Ok(next) = outcome {
        return Some(next);
    }
    // Unclosed block comments are consumed to EOF; failed regex is not noise.
    (bytes.get(i + 1) == Some(&b'*')).then_some(bytes.len())
}

fn try_skip_slash_token(bytes: &[u8], i: usize) -> Option<Result<usize, &'static str>> {
    let next = bytes.get(i + 1).copied()?;
    if next == b'/' {
        return Some(Ok(skip_line_comment(bytes, i)));
    }
    if next == b'*' {
        return Some(try_skip_block_comment(bytes, i).ok_or("unclosed comment"));
    }
    if can_start_regex(bytes, i) {
        return Some(skip_regex(bytes, i).ok_or("unclosed regex"));
    }
    None
}

fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn try_skip_block_comment(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 2;
    while j + 1 < bytes.len() {
        if bytes[j] == b'*' && bytes[j + 1] == b'/' {
            return Some(j + 2);
        }
        j += 1;
    }
    None
}

/// Skips a `"..."`, `'...'`, or `` `...` `` literal starting at `start`.
pub(super) fn skip_string(bytes: &[u8], start: usize) -> usize {
    try_skip_string(bytes, start).unwrap_or(bytes.len())
}

fn try_skip_string(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = *bytes.get(start)?;
    if quote == b'`' {
        return try_skip_template(bytes, start);
    }
    try_skip_quoted_string(bytes, start, quote)
}

fn try_skip_template(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match try_template_byte(bytes, i) {
            TemplateOut::Done(end) => return Some(end),
            TemplateOut::Next(next) => i = next,
        }
    }
    None
}

enum TemplateOut {
    Done(usize),
    Next(usize),
}

fn try_template_byte(bytes: &[u8], i: usize) -> TemplateOut {
    if bytes[i] == b'`' {
        return TemplateOut::Done(i + 1);
    }
    if let Some(next) = template_interpolation(bytes, i) {
        return TemplateOut::Next(next);
    }
    if let Some(next) = template_escape(bytes, i) {
        return TemplateOut::Next(next);
    }
    TemplateOut::Next(i + 1)
}

fn template_interpolation(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] != b'$' || bytes.get(i + 1) != Some(&b'{') {
        return None;
    }
    skip_balanced(bytes, i + 1, b'{', b'}')
}

fn template_escape(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] != b'\\' {
        return None;
    }
    (i + 1 < bytes.len()).then_some(i + 2)
}

fn try_skip_quoted_string(bytes: &[u8], start: usize, quote: u8) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            return Some(i + 1);
        }
        i = after_quoted_byte(bytes, i)?;
    }
    None
}

fn after_quoted_byte(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] != b'\\' {
        return Some(i + 1);
    }
    (i + 1 < bytes.len()).then_some(i + 2)
}

pub(super) fn prev_code_byte(bytes: &[u8], i: usize) -> Option<u8> {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if !bytes[j].is_ascii_whitespace() {
            return Some(bytes[j]);
        }
    }
    None
}
pub(super) fn prev_non_ws(bytes: &[u8], i: usize) -> usize {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if !bytes[j].is_ascii_whitespace() {
            return j + 1;
        }
    }
    0
}

pub(super) fn ident_start_before(bytes: &[u8], end: usize) -> usize {
    let mut j = end;
    while j > 0 && is_ident_byte(bytes[j - 1]) {
        j -= 1;
    }
    j
}

/// Skips a balanced `open_ch`…`close_ch` pair starting at `open`.
pub(super) fn skip_balanced(bytes: &[u8], open: usize, open_ch: u8, close_ch: u8) -> Option<usize> {
    if bytes.get(open) != Some(&open_ch) {
        return None;
    }
    scan_balanced(bytes, open, open_ch, close_ch)
}

fn scan_balanced(bytes: &[u8], open: usize, open_ch: u8, close_ch: u8) -> Option<usize> {
    let mut depth = 0_i32;
    let mut i = open;
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        if let Some(end) = step_balance(&mut depth, bytes[i], open_ch, close_ch, i) {
            return Some(end);
        }
        i += 1;
    }
    None
}

fn step_balance(depth: &mut i32, b: u8, open_ch: u8, close_ch: u8, i: usize) -> Option<usize> {
    if b == open_ch {
        *depth += 1;
        return None;
    }
    if b != close_ch {
        return None;
    }
    *depth -= 1;
    (*depth == 0).then_some(i + 1)
}

/// Returns whether `op` matches the bytes at `i`.
pub(super) fn match_op(bytes: &[u8], i: usize, op: &[u8]) -> bool {
    bytes.get(i..i + op.len()) == Some(op)
}

/// Returns whether `word` is a whole identifier at `i`.
pub(super) fn is_word(bytes: &[u8], i: usize, word: &[u8]) -> bool {
    bytes.get(i..i + word.len()) == Some(word)
        && !is_ident_byte(bytes.get(i.wrapping_sub(1)).copied().unwrap_or(0))
        && !is_ident_byte(bytes.get(i + word.len()).copied().unwrap_or(0))
}

/// Returns whether `b` can continue a TypeScript identifier.
pub(super) const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Returns whether `b` can start a TypeScript identifier.
pub(super) const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

#[cfg(test)]
#[path = "../lex_tests.rs"]
mod tests;
