//! Scan Go source for named functions and their body spans.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::count_metric;
use super::lex::{is_ident_byte, is_ident_start};
use super::structure::validate_structure;
use crap_core::{Error, FunctionComplexity, Metric, Result};
use std::path::Path;

pub(super) use super::lex::skip_balanced;

/// Named function found in a Go source file.
#[derive(Debug, Clone)]
struct FoundFn {
    name: String,
    start_line: usize,
    end_line: usize,
    body_lo: usize,
    body_hi: usize,
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
    // Mask from the end so earlier offsets stay valid.
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
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if i >= bytes.len() {
            break;
        }
        if is_func_keyword(bytes, i) {
            if let Some(func) = try_parse_func(source, bytes, i) {
                out.push(func);
            }
            // Continue inside the body so nested named funcs are found.
            i += 4;
            continue;
        }
        i += 1;
    }
    out
}

fn try_parse_func(source: &str, bytes: &[u8], start: usize) -> Option<FoundFn> {
    if !is_func_keyword(bytes, start) {
        return None;
    }
    let (name, body_lo, body_hi) = parse_func_parts(source, bytes, start + 4)?;
    Some(FoundFn {
        name,
        start_line: line_of(source, start),
        end_line: line_of(source, body_hi.saturating_sub(1)),
        body_lo,
        body_hi,
    })
}

fn parse_func_parts(
    source: &str,
    bytes: &[u8],
    after_func: usize,
) -> Option<(String, usize, usize)> {
    let mut i = skip_spaces(bytes, after_func);
    let (name, after_sig) = parse_func_name(source, bytes, i)?;
    i = skip_spaces(bytes, after_sig);
    i = skip_signature(bytes, i)?;
    parse_func_body(bytes, i).map(|(lo, hi)| (name, lo, hi))
}

fn parse_func_body(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let i = skip_spaces(bytes, i);
    if bytes.get(i) != Some(&b'{') {
        return None;
    }
    let body_hi = match_braces(bytes, i)?;
    Some((i, body_hi))
}

fn is_func_keyword(bytes: &[u8], i: usize) -> bool {
    bytes.get(i..i + 4) == Some(b"func")
        && !is_ident_byte(bytes.get(i.wrapping_sub(1)).copied().unwrap_or(0))
        && !is_ident_byte(bytes.get(i + 4).copied().unwrap_or(0))
}

fn parse_func_name(source: &str, bytes: &[u8], i: usize) -> Option<(String, usize)> {
    if bytes.get(i) == Some(&b'(') {
        return parse_method_name(source, bytes, i);
    }
    let (name, next) = read_ident(source, bytes, i)?;
    Some((name, next))
}

fn parse_method_name(source: &str, bytes: &[u8], i: usize) -> Option<(String, usize)> {
    // `func (recv *T) Name` — skip receiver, then read Name.
    // Closures are `func (` ... `)` without a following identifier.
    let after_recv = skip_balanced(bytes, i, b'(', b')')?;
    let after = skip_spaces(bytes, after_recv);
    let Some((name, next)) = read_ident(source, bytes, after) else {
        // Function literal / closure: not a separate row.
        return None;
    };
    let recv = receiver_type(source, bytes, i + 1, after_recv.saturating_sub(1));
    let full = recv.map_or_else(|| name.clone(), |ty| format!("{ty}.{name}"));
    Some((full, next))
}

fn receiver_type(source: &str, bytes: &[u8], lo: usize, hi: usize) -> Option<String> {
    let i = after_optional_ident(source, bytes, lo);
    scan_receiver_type(source, bytes, i, hi)
}

fn after_optional_ident(source: &str, bytes: &[u8], lo: usize) -> usize {
    let i = skip_spaces(bytes, lo);
    if let Some((_, next)) = read_ident(source, bytes, i) {
        return skip_spaces(bytes, next);
    }
    i
}

fn scan_receiver_type(source: &str, bytes: &[u8], start: usize, hi: usize) -> Option<String> {
    let end = hi.min(bytes.len());
    let mut i = start;
    while i < end {
        let b = bytes[i];
        if matches!(b, b'*' | b' ' | b'\t') {
            i += 1;
            continue;
        }
        if is_ident_byte(b) {
            return read_ident(source, bytes, i).map(|(ty, _)| ty);
        }
        break;
    }
    None
}

fn skip_signature(bytes: &[u8], i: usize) -> Option<usize> {
    // Params `(...)` then optional result (type, `*T`, or `(...)`).
    let mut i = skip_spaces(bytes, i);
    if bytes.get(i) != Some(&b'(') {
        return None;
    }
    i = skip_balanced(bytes, i, b'(', b')')?;
    i = skip_spaces(bytes, i);
    skip_result_type(bytes, i)
}

fn skip_result_type(bytes: &[u8], i: usize) -> Option<usize> {
    // EOF behaves like `{` (result omitted; body starts / end of input).
    let b = bytes.get(i).copied().unwrap_or(b'{');
    skip_result_byte(bytes, i, b)
}

fn skip_result_byte(bytes: &[u8], i: usize, b: u8) -> Option<usize> {
    if b == b'{' {
        return Some(i);
    }
    if b == b'(' {
        return skip_balanced(bytes, i, b'(', b')');
    }
    if starts_typeish(b) {
        return Some(skip_typeish(bytes, i));
    }
    Some(i)
}

const fn starts_typeish(b: u8) -> bool {
    is_ident_byte(b) || b == b'*' || b == b'['
}

fn skip_typeish(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if matches!(bytes[i], b'{' | b'\n') {
            break;
        }
        if let Some(next) = skip_typeish_bracket(bytes, i) {
            i = next;
            continue;
        }
        i += 1;
    }
    i
}

fn skip_typeish_bracket(bytes: &[u8], i: usize) -> Option<usize> {
    match bytes[i] {
        b'(' => skip_balanced(bytes, i, b'(', b')'),
        b'[' => skip_balanced(bytes, i, b'[', b']'),
        _ => None,
    }
}

fn match_braces(bytes: &[u8], open: usize) -> Option<usize> {
    skip_balanced(bytes, open, b'{', b'}')
}

fn skip_noise(bytes: &[u8], i: usize) -> usize {
    super::lex::skip_noise(bytes, i)
}

fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

fn read_ident(source: &str, bytes: &[u8], i: usize) -> Option<(String, usize)> {
    if !bytes.get(i).is_some_and(|&b| is_ident_start(b)) {
        return None;
    }
    let mut end = i + 1;
    while bytes.get(end).is_some_and(|&b| is_ident_byte(b)) {
        end += 1;
    }
    Some((source[i..end].to_owned(), end))
}

fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].bytes().filter(|&b| b == b'\n').count() + 1
}

#[cfg(test)]
#[path = "visitor_tests.rs"]
mod tests;
