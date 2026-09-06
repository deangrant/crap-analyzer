//! Cognitive complexity for a Go function body.

/// Returns cognitive complexity for `body` (minimum 0).
pub(super) fn count(body: &str) -> usize {
    let mut counter = CognitiveCounter {
        count: 0,
        nesting: 0,
    };
    counter.scan(body.as_bytes());
    counter.count
}

struct CognitiveCounter {
    count: usize,
    nesting: usize,
}

impl CognitiveCounter {
    const fn add_nested(&mut self) {
        self.count += 1 + self.nesting;
    }

    fn scan(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i < bytes.len() {
            i = skip_noise(bytes, i);
            if i >= bytes.len() {
                break;
            }
            if let Some(next) = self.take_token(bytes, i) {
                i = next;
                continue;
            }
            i += 1;
        }
    }

    fn take_token(&mut self, bytes: &[u8], i: usize) -> Option<usize> {
        if let Some(end) = consume_bool_run(bytes, i) {
            self.count += 1;
            return Some(end);
        }
        if is_word(bytes, i, b"else") {
            self.count += 1;
            return Some(i + 4);
        }
        for word in [b"if" as &[u8], b"for", b"switch", b"select"] {
            if is_word(bytes, i, word) {
                self.add_nested();
                return Some(self.after_control(bytes, i + word.len()));
            }
        }
        None
    }

    fn after_control(&mut self, bytes: &[u8], start: usize) -> usize {
        let Some(open) = find_block_open(bytes, start) else {
            return start;
        };
        self.scan_bools(&bytes[start..open]);
        let Some(end) = match_braces(bytes, open) else {
            return open + 1;
        };
        if end > open + 1 {
            self.nesting += 1;
            self.scan(&bytes[open + 1..end - 1]);
            self.nesting = self.nesting.saturating_sub(1);
        }
        end
    }

    fn scan_bools(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i < bytes.len() {
            i = skip_noise(bytes, i);
            if i < bytes.len() {
                if let Some(end) = consume_bool_run(bytes, i) {
                    self.count += 1;
                    i = end;
                } else {
                    i += 1;
                }
            }
        }
    }
}

fn consume_bool_run(bytes: &[u8], i: usize) -> Option<usize> {
    let op: &[u8] = if match_op(bytes, i, b"&&") {
        b"&&"
    } else if match_op(bytes, i, b"||") {
        b"||"
    } else {
        return None;
    };
    let mut i = i + op.len();
    loop {
        i = skip_to_bool_or_stop(bytes, i);
        if match_op(bytes, i, op) {
            i += op.len();
            continue;
        }
        return Some(i);
    }
}

fn skip_to_bool_or_stop(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if i >= bytes.len() {
            return i;
        }
        if match_op(bytes, i, b"&&") || match_op(bytes, i, b"||") {
            return i;
        }
        if matches!(bytes[i], b';' | b'{' | b')' | b',') {
            return i;
        }
        i += 1;
    }
    i
}

fn find_block_open(bytes: &[u8], mut i: usize) -> Option<usize> {
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b'{' {
            return Some(i);
        }
        if bytes[i] == b';' {
            return None;
        }
        i += 1;
    }
    None
}

fn match_op(bytes: &[u8], i: usize, op: &[u8]) -> bool {
    bytes.get(i..i + op.len()) == Some(op)
}

fn is_word(bytes: &[u8], i: usize, word: &[u8]) -> bool {
    bytes.get(i..i + word.len()) == Some(word)
        && !is_ident_byte(bytes.get(i.wrapping_sub(1)).copied().unwrap_or(0))
        && !is_ident_byte(bytes.get(i + word.len()).copied().unwrap_or(0))
}

fn match_braces(bytes: &[u8], open: usize) -> Option<usize> {
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 0_i32;
    let mut i = open;
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_noise(bytes: &[u8], i: usize) -> usize {
    super::lex::skip_noise(bytes, i)
}

const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
#[path = "cognitive_tests.rs"]
mod tests;
