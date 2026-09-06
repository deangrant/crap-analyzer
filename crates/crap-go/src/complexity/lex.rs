//! Shared scanners for Go source tokens (comments and strings).

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
    match bytes[i] {
        b'/' if bytes.get(i + 1) == Some(&b'/') => Some(skip_line_comment(bytes, i)),
        b'/' if bytes.get(i + 1) == Some(&b'*') => Some(skip_block_comment(bytes, i)),
        b'"' | b'\'' | b'`' => Some(skip_string(bytes, i)),
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
        match bytes[i] {
            b'\\' => i = i.saturating_add(2),
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    i
}
