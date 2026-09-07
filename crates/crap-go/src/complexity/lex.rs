//! Shared scanners for Go source tokens (comments, strings, braces, and words).

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
    match bytes.get(i + 1) {
        Some(&b'/') => Some(skip_line_comment(bytes, i)),
        Some(&b'*') => Some(skip_block_comment(bytes, i)),
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

fn skip_block_comment(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
    }
    i.saturating_add(2)
}

/// Skips a `"..."`, `'...'`, or `` `...` `` literal starting at `start`.
pub(super) fn skip_string(bytes: &[u8], start: usize) -> usize {
    let Some(&quote) = bytes.get(start) else {
        return start;
    };
    if quote == b'`' {
        return skip_raw_string(bytes, start);
    }
    skip_quoted_string(bytes, start, quote)
}

fn skip_raw_string(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != b'`' {
        i += 1;
    }
    i.saturating_add(1)
}

fn skip_quoted_string(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            return i + 1;
        }
        i = after_quoted_byte(bytes, i);
    }
    i
}

fn after_quoted_byte(bytes: &[u8], i: usize) -> usize {
    if bytes[i] == b'\\' {
        return i.saturating_add(2);
    }
    i + 1
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
