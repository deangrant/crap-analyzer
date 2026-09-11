//! Shared scanners for Python tokens (comments, strings, braces, and words).

/// Skips spaces, comments, and string literals starting at `i`.
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
    if bytes[i] == b'#' {
        return Some(skip_line_comment(bytes, i));
    }
    if is_string_prefix_or_quote(bytes, i) {
        return Some(skip_string(bytes, i));
    }
    None
}

/// Tries to skip one comment or string token at `i`.
///
/// Returns `None` when `i` is not noise. `Err` means an unclosed literal.
pub(super) fn try_skip_noise_token(bytes: &[u8], i: usize) -> Option<Result<usize, &'static str>> {
    if bytes.get(i) == Some(&b'#') {
        return Some(Ok(skip_line_comment(bytes, i)));
    }
    if is_string_prefix_or_quote(bytes, i) {
        return Some(try_skip_string(bytes, i).ok_or("unclosed string"));
    }
    None
}

fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn is_string_prefix_or_quote(bytes: &[u8], i: usize) -> bool {
    matches!(bytes.get(i), Some(&b'"' | &b'\''))
        || string_prefix_len(bytes, i)
            .is_some_and(|n| matches!(bytes.get(i + n), Some(&b'"' | &b'\'')))
}

/// Length of an optional string prefix (`r`, `f`, `b`, `u`, and pairs) at `i`.
fn string_prefix_len(bytes: &[u8], i: usize) -> Option<usize> {
    let b0 = bytes.get(i).copied()?.to_ascii_lowercase();
    if !matches!(b0, b'r' | b'f' | b'b' | b'u') {
        return None;
    }
    let b1 = bytes.get(i + 1).map(u8::to_ascii_lowercase);
    if matches!(
        (b0, b1),
        (b'r', Some(b'f' | b'b')) | (b'f' | b'b', Some(b'r'))
    ) {
        return Some(2);
    }
    Some(1)
}

/// Skips a Python string literal starting at `start`.
pub(super) fn skip_string(bytes: &[u8], start: usize) -> usize {
    try_skip_string(bytes, start).unwrap_or(bytes.len())
}

fn try_skip_string(bytes: &[u8], start: usize) -> Option<usize> {
    let prefix = string_prefix_len(bytes, start).unwrap_or(0);
    let quote_at = start + prefix;
    let quote = *bytes.get(quote_at)?;
    if !matches!(quote, b'"' | b'\'') {
        return None;
    }
    if is_triple_quote(bytes, quote_at, quote) {
        return try_skip_triple(bytes, quote_at, quote);
    }
    try_skip_quoted_string(bytes, quote_at, quote)
}

fn is_triple_quote(bytes: &[u8], quote_at: usize, quote: u8) -> bool {
    bytes.get(quote_at + 1) == Some(&quote) && bytes.get(quote_at + 2) == Some(&quote)
}

fn try_skip_triple(bytes: &[u8], quote_at: usize, quote: u8) -> Option<usize> {
    let mut i = quote_at + 3;
    while i + 2 < bytes.len() {
        match step_triple(bytes, i, quote) {
            TripleStep::Done(end) => return Some(end),
            TripleStep::Next(next) => i = next,
        }
    }
    None
}

enum TripleStep {
    Done(usize),
    Next(usize),
}

fn step_triple(bytes: &[u8], i: usize, quote: u8) -> TripleStep {
    if bytes[i] == quote && bytes[i + 1] == quote && bytes[i + 2] == quote {
        return TripleStep::Done(i + 3);
    }
    if bytes[i] == b'\\' {
        return TripleStep::Next(i.saturating_add(2));
    }
    TripleStep::Next(i + 1)
}

fn try_skip_quoted_string(bytes: &[u8], start: usize, quote: u8) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            return Some(i + 1);
        }
        if bytes[i] == b'\n' {
            return None;
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

/// Returns whether `word` is a whole identifier at `i`.
pub(super) fn is_word(bytes: &[u8], i: usize, word: &[u8]) -> bool {
    bytes.get(i..i + word.len()) == Some(word)
        && !is_ident_byte(bytes.get(i.wrapping_sub(1)).copied().unwrap_or(0))
        && !is_ident_byte(bytes.get(i + word.len()).copied().unwrap_or(0))
}

/// Returns whether `b` can continue a Python identifier.
pub(super) const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Returns whether `b` can start a Python identifier.
pub(super) const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[cfg(test)]
#[path = "lex_tests.rs"]
mod tests;
