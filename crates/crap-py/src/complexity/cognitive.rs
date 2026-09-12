//! Cognitive complexity for a Python function body.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::lex::{is_word, skip_noise};
use super::lines::{is_blank_or_comment_at, leading_indent, line_end};

/// Returns cognitive complexity for `body` (minimum 0).
pub(super) fn count(body: &str) -> usize {
    let lines = body_lines(body.as_bytes());
    let mut counter = CognitiveCounter {
        count: 0,
        nest_indents: Vec::new(),
    };
    for line in &lines {
        counter.take_line(body.as_bytes(), *line);
    }
    counter.count
}

#[derive(Clone, Copy)]
struct BodyLine {
    indent: usize,
    content: usize,
    end: usize,
}

struct CognitiveCounter {
    count: usize,
    nest_indents: Vec<usize>,
}

impl CognitiveCounter {
    fn take_line(&mut self, bytes: &[u8], line: BodyLine) {
        let Some(i) = statement_start(bytes, line) else {
            return;
        };
        self.pop_left_blocks(line.indent);
        self.take_statement(bytes, i, line);
    }

    fn pop_left_blocks(&mut self, indent: usize) {
        while self.nest_indents.last().is_some_and(|n| indent <= *n) {
            self.nest_indents.pop();
        }
    }

    fn take_statement(&mut self, bytes: &[u8], i: usize, line: BodyLine) {
        if take_flat_control(bytes, i) {
            self.count += 1;
            self.scan_bools(bytes, i, line.end);
            return;
        }
        if let Some(word_len) = nesting_control_len(bytes, i) {
            self.count += 1 + self.nest_indents.len();
            self.nest_indents.push(line.indent);
            self.scan_bools(bytes, i + word_len, line.end);
            return;
        }
        self.scan_bools(bytes, i, line.end);
    }

    fn scan_bools(&mut self, bytes: &[u8], start: usize, end: usize) {
        let mut i = start;
        while i < end && i < bytes.len() {
            i = advance_bool_scan(bytes, i, end, &mut self.count);
        }
    }
}

fn take_flat_control(bytes: &[u8], i: usize) -> bool {
    is_word(bytes, i, b"elif") || is_word(bytes, i, b"else") || is_word(bytes, i, b"except")
}

fn statement_start(bytes: &[u8], line: BodyLine) -> Option<usize> {
    if is_blank_or_comment_at(bytes, line.content) {
        return None;
    }
    let i = skip_noise(bytes, line.content);
    (i < line.end && i < bytes.len()).then_some(i)
}

fn nesting_control_len(bytes: &[u8], i: usize) -> Option<usize> {
    for word in [b"if" as &[u8], b"for", b"while", b"try", b"with", b"match"] {
        if is_word(bytes, i, word) {
            return Some(word.len());
        }
    }
    None
}

fn advance_bool_scan(bytes: &[u8], i: usize, end: usize, count: &mut usize) -> usize {
    let i = skip_noise(bytes, i);
    if i >= end || i >= bytes.len() {
        return end;
    }
    if let Some(next) = consume_bool_run(bytes, i) {
        *count += 1;
        return next;
    }
    i + 1
}

fn consume_bool_run(bytes: &[u8], i: usize) -> Option<usize> {
    let op = bool_op_at(bytes, i)?;
    let mut i = i + op.len();
    loop {
        i = skip_to_bool_or_stop(bytes, i);
        if match_same_op(bytes, i, op) {
            i += op.len();
            continue;
        }
        return Some(i);
    }
}

fn bool_op_at(bytes: &[u8], i: usize) -> Option<&'static [u8]> {
    if is_word(bytes, i, b"and") {
        return Some(b"and");
    }
    if is_word(bytes, i, b"or") {
        return Some(b"or");
    }
    None
}

fn match_same_op(bytes: &[u8], i: usize, op: &[u8]) -> bool {
    is_word(bytes, i, op)
}

fn skip_to_bool_or_stop(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if at_bool_or_stop(bytes, i) {
            return i;
        }
        i += 1;
    }
    i
}

fn at_bool_or_stop(bytes: &[u8], i: usize) -> bool {
    if i >= bytes.len() {
        return true;
    }
    if is_word(bytes, i, b"and") || is_word(bytes, i, b"or") {
        return true;
    }
    matches!(bytes[i], b'\n' | b':' | b')' | b',' | b';')
}

fn body_lines(bytes: &[u8]) -> Vec<BodyLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start <= bytes.len() {
        let end = line_end(bytes, start);
        let (indent, content) = leading_indent(bytes, start, end).unwrap_or((0, start));
        lines.push(BodyLine {
            indent,
            content,
            end,
        });
        if end >= bytes.len() {
            break;
        }
        start = end;
    }
    lines
}

#[cfg(test)]
#[path = "cognitive_tests.rs"]
mod tests;
