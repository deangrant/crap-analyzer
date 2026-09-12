//! Scan Python source for named functions and their body spans.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::count_metric;
use super::lex::{is_ident_byte, is_ident_start, is_word, skip_noise};
use super::lines::{is_blank_or_comment_at, leading_indent, line_end};
use super::structure::validate_structure;
use crap_core::{Error, FunctionComplexity, Metric, Result};
use std::path::Path;

/// Named function found in a Python source file.
#[derive(Debug, Clone)]
pub(super) struct FoundFn {
    pub(super) name: String,
    pub(super) start_line: usize,
    pub(super) end_line: usize,
    pub(super) body_lo: usize,
    pub(super) body_hi: usize,
}

/// One physical line with indent and content offsets.
#[derive(Debug, Clone, Copy)]
struct Line {
    /// One-based line number.
    number: usize,
    /// Leading whitespace width (spaces; tab counts as 1 for simplicity).
    indent: usize,
    /// Byte offset of the first non-whitespace byte, or line end.
    content: usize,
    /// Exclusive end offset of the line (including newline when present).
    end: usize,
}

/// Parses `source` as if it lived at `path`.
///
/// # Errors
///
/// Returns [`Error::Collect`] when the source fails the structural integrity
/// scan (unclosed literals/comments or unbalanced braces).
pub fn analyze_source(
    path: &Path,
    source: &str,
    metric: Metric,
) -> Result<Vec<FunctionComplexity>> {
    if let Err(reason) = validate_structure(source.as_bytes()) {
        return Err(Error::collect(format!("{}: {reason}", path.display())));
    }
    let found = find_functions(source);
    Ok(found
        .iter()
        .map(|func| to_complexity(path, source, metric, func, &found))
        .collect())
}

fn to_complexity(
    path: &Path,
    source: &str,
    metric: Metric,
    func: &FoundFn,
    all: &[FoundFn],
) -> FunctionComplexity {
    let body = masked_body(source, func, all);
    FunctionComplexity {
        file: path.to_path_buf(),
        name: func.name.clone(),
        start_line: func.start_line,
        end_line: func.end_line,
        complexity: count_metric(metric, &body),
    }
}

fn masked_body(source: &str, func: &FoundFn, all: &[FoundFn]) -> String {
    let mut body = source[func.body_lo..func.body_hi].to_owned();
    let nested: Vec<_> = all.iter().filter(|other| is_nested(func, other)).collect();
    for other in nested.into_iter().rev() {
        let lo = other.body_lo.saturating_sub(func.body_lo);
        let hi = other.body_hi.saturating_sub(func.body_lo).min(body.len());
        if lo < hi {
            body.replace_range(lo..hi, &" ".repeat(hi - lo));
        }
    }
    body
}

const fn is_nested(outer: &FoundFn, inner: &FoundFn) -> bool {
    let smaller = inner.start_line > outer.start_line || inner.end_line < outer.end_line;
    outer.body_lo <= inner.body_lo && inner.body_hi <= outer.body_hi && smaller
}

fn find_functions(source: &str) -> Vec<FoundFn> {
    let bytes = source.as_bytes();
    let lines = split_lines(bytes);
    let mut out = Vec::new();
    let mut class_stack: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        i = take_line(source, bytes, &lines, i, &mut class_stack, &mut out);
    }
    out
}

fn take_line(
    source: &str,
    bytes: &[u8],
    lines: &[Line],
    index: usize,
    class_stack: &mut Vec<(usize, String)>,
    out: &mut Vec<FoundFn>,
) -> usize {
    let line = lines[index];
    pop_classes(class_stack, line.indent, line.content, bytes);
    if is_blank_or_comment(bytes, line) {
        return index + 1;
    }
    if try_take_class(source, bytes, lines, index, class_stack) {
        return index + 1;
    }
    if try_take_def(source, bytes, lines, index, class_stack, out) {
        return index + 1;
    }
    index + 1
}

fn pop_classes(
    class_stack: &mut Vec<(usize, String)>,
    indent: usize,
    content: usize,
    bytes: &[u8],
) {
    if content >= bytes.len() || is_blank_or_comment_at(bytes, content) {
        return;
    }
    while class_stack.last().is_some_and(|&(class_indent, _)| indent <= class_indent) {
        class_stack.pop();
    }
}

fn try_take_class(
    source: &str,
    bytes: &[u8],
    lines: &[Line],
    index: usize,
    class_stack: &mut Vec<(usize, String)>,
) -> bool {
    let line = lines[index];
    let j = line.content;
    if bytes.get(j) == Some(&b'@') {
        return false;
    }
    if !is_word(bytes, j, b"class") {
        return false;
    }
    let j = skip_spaces(bytes, j + 5);
    let Some((name, _)) = read_ident(source, bytes, j) else {
        return false;
    };
    class_stack.push((line.indent, name));
    true
}

fn try_take_def(
    source: &str,
    bytes: &[u8],
    lines: &[Line],
    index: usize,
    class_stack: &[(usize, String)],
    out: &mut Vec<FoundFn>,
) -> bool {
    let line = lines[index];
    let Some((def_start, name, colon)) = parse_def_header(source, bytes, line) else {
        return false;
    };
    let qualified = qualify_name(class_stack, line.indent, &name);
    let (end_line, body_hi) = suite_end(bytes, lines, line.indent, colon + 1);
    out.push(FoundFn {
        name: qualified,
        start_line: line.number,
        end_line,
        body_lo: def_start,
        body_hi,
    });
    true
}

fn parse_def_header(source: &str, bytes: &[u8], line: Line) -> Option<(usize, String, usize)> {
    let j = line.content;
    if bytes.get(j) == Some(&b'@') {
        return None;
    }
    let after_kw = skip_def_keyword(bytes, j)?;
    let (name, after_name) = read_ident(source, bytes, after_kw)?;
    let colon = find_def_colon(bytes, after_name)?;
    Some((j, name, colon))
}

fn skip_def_keyword(bytes: &[u8], mut j: usize) -> Option<usize> {
    if is_word(bytes, j, b"async") {
        j = skip_spaces(bytes, j + 5);
    }
    if !is_word(bytes, j, b"def") {
        return None;
    }
    Some(skip_spaces(bytes, j + 3))
}

fn qualify_name(class_stack: &[(usize, String)], indent: usize, name: &str) -> String {
    let Some((_, class_name)) =
        class_stack.iter().rev().find(|(class_indent, _)| indent > *class_indent)
    else {
        return name.to_owned();
    };
    format!("{class_name}.{name}")
}

fn find_def_colon(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    let mut depth = 0_usize;
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if i >= bytes.len() {
            return None;
        }
        match sig_byte(bytes[i], depth) {
            SigAction::Found => return Some(i),
            SigAction::Stop => return None,
            SigAction::Open => depth += 1,
            SigAction::Close => depth = depth.saturating_sub(1),
            SigAction::Other => {}
        }
        i += 1;
    }
    None
}

#[derive(Clone, Copy)]
enum SigAction {
    Found,
    Stop,
    Open,
    Close,
    Other,
}

const fn sig_byte(b: u8, depth: usize) -> SigAction {
    match b {
        b':' if depth == 0 => SigAction::Found,
        b'\n' if depth == 0 => SigAction::Stop,
        b'(' | b'[' => SigAction::Open,
        b')' | b']' => SigAction::Close,
        _ => SigAction::Other,
    }
}

fn suite_end(
    bytes: &[u8],
    lines: &[Line],
    def_indent: usize,
    after_colon: usize,
) -> (usize, usize) {
    let colon_index = line_index_at(lines, after_colon - 1);
    let colon_line = lines[colon_index];
    if has_inline_suite(bytes, after_colon, colon_line.end) {
        return (colon_line.number, colon_line.end.min(bytes.len()));
    }
    let mut end_line = colon_line.number;
    let mut body_hi = colon_line.end.min(bytes.len());
    for line in lines.iter().skip(colon_index + 1) {
        if is_blank_or_comment(bytes, *line) {
            body_hi = line.end.min(bytes.len());
            continue;
        }
        if line.indent <= def_indent {
            break;
        }
        end_line = line.number;
        body_hi = line.end.min(bytes.len());
    }
    (end_line, body_hi)
}

fn line_index_at(lines: &[Line], offset: usize) -> usize {
    lines
        .iter()
        .position(|line| offset < line.end)
        .unwrap_or_else(|| lines.len().saturating_sub(1))
}

fn has_inline_suite(bytes: &[u8], after_colon: usize, line_end: usize) -> bool {
    let i = skip_spaces(bytes, after_colon);
    i < line_end && i < bytes.len() && bytes[i] != b'#' && bytes[i] != b'\n'
}

fn is_blank_or_comment(bytes: &[u8], line: Line) -> bool {
    is_blank_or_comment_at(bytes, line.content)
}

fn split_lines(bytes: &[u8]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut number = 1_usize;
    while start <= bytes.len() {
        let end = line_end(bytes, start);
        lines.push(make_line(bytes, number, start, end));
        if end >= bytes.len() {
            break;
        }
        start = end;
        number += 1;
    }
    lines
}

fn make_line(bytes: &[u8], number: usize, start: usize, end: usize) -> Line {
    let (indent, content) = leading_indent(bytes, start, end);
    Line {
        number,
        indent,
        content,
        end,
    }
}

fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
        i += 1;
    }
    i
}

fn read_ident(source: &str, bytes: &[u8], i: usize) -> Option<(String, usize)> {
    if !is_ident_start(bytes.get(i).copied()?) {
        return None;
    }
    let mut j = i + 1;
    while j < bytes.len() && is_ident_byte(bytes[j]) {
        j += 1;
    }
    Some((source[i..j].to_owned(), j))
}

#[cfg(test)]
#[path = "visitor_tests.rs"]
mod tests;
