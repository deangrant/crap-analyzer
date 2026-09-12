//! Shared scanners for Go source tokens (comments, strings, braces, and words).

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
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
    if bytes[i] == b'/' {
        return skip_comment_start(bytes, i);
    }
    if matches!(bytes[i], b'"' | b'\'' | b'`') {
        return Some(skip_string(bytes, i));
    }
    None
}

fn skip_comment_start(bytes: &[u8], i: usize) -> Option<usize> {
    let outcome = try_skip_comment_token(bytes, i)?;
    if let Ok(next) = outcome {
        return Some(next);
    }
    Some(bytes.len())
}

/// Tries to skip one comment or string token at `i`.
///
/// Returns `None` when `i` is not noise. `Err` means an unclosed literal.
pub(super) fn try_skip_noise_token(bytes: &[u8], i: usize) -> Option<Result<usize, &'static str>> {
    if bytes.get(i) == Some(&b'/') {
        return try_skip_comment_token(bytes, i);
    }
    if matches!(bytes.get(i), Some(&b'"' | &b'\'' | &b'`')) {
        return Some(try_skip_string(bytes, i).ok_or("unclosed string"));
    }
    None
}

fn try_skip_comment_token(bytes: &[u8], i: usize) -> Option<Result<usize, &'static str>> {
    match bytes.get(i + 1) {
        Some(&b'/') => Some(Ok(skip_line_comment(bytes, i))),
        Some(&b'*') => Some(try_skip_block_comment(bytes, i).ok_or("unclosed comment")),
        _ => None,
    }
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
        return try_skip_raw_string(bytes, start);
    }
    try_skip_quoted_string(bytes, start, quote)
}

fn try_skip_raw_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            return Some(i + 1);
        }
        i += 1;
    }
    None
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

/// Returns whether `b` can continue a Go identifier.
pub(super) const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Returns whether `b` can start a Go identifier.
pub(super) const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::skip_noise;

    #[test]
    fn lone_slash_is_not_noise() {
        assert_eq!(skip_noise(b"/", 0), 0);
        assert_eq!(skip_noise(b"/x", 0), 0);
    }
}
