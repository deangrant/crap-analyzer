//! Scan TypeScript source for named functions and their body spans.

#[path = "visitor_class.rs"]
mod class;

use super::count_metric;
use super::lex::{is_ident_byte, is_ident_start, is_word, skip_balanced, skip_noise};
use crap_core::{FunctionComplexity, Metric};
use std::path::Path;

/// Named function found in a TypeScript source file.
#[derive(Debug, Clone)]
pub(super) struct FoundFn {
    pub(super) name: String,
    pub(super) start_line: usize,
    pub(super) end_line: usize,
    pub(super) body_lo: usize,
    pub(super) body_hi: usize,
}

/// Parses `source` as if it lived at `path`.
#[must_use]
pub fn analyze_source(path: &Path, source: &str, metric: Metric) -> Vec<FunctionComplexity> {
    let found = find_functions(source);
    found
        .iter()
        .map(|func| to_complexity(path, source, metric, func, &found))
        .collect()
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
        i = take_next_form(source, bytes, i, &mut out);
    }
    out
}

fn take_next_form(source: &str, bytes: &[u8], i: usize, out: &mut Vec<FoundFn>) -> usize {
    if i >= bytes.len() {
        return i;
    }
    let mut cur = i;
    if try_take_declaration(source, bytes, &mut cur, out) {
        return cur;
    }
    if class::try_take_class(source, bytes, &mut cur, out) {
        return cur;
    }
    if try_take_assigned(source, bytes, &mut cur, out) {
        return cur;
    }
    i + 1
}

fn try_take_declaration(source: &str, bytes: &[u8], i: &mut usize, out: &mut Vec<FoundFn>) -> bool {
    let start = *i;
    let mut j = start;
    j = skip_export_declare(bytes, j);
    let async_start = j;
    if is_word(bytes, j, b"async") {
        j = skip_spaces(bytes, j + 5);
    }
    if !is_word(bytes, j, b"function") {
        return false;
    }
    // Ambient / overload signatures have no `{` body.
    if let Some(func) = parse_function_decl(source, bytes, async_start) {
        out.push(func);
    }
    *i = j + 8;
    true
}

fn try_take_assigned(source: &str, bytes: &[u8], i: &mut usize, out: &mut Vec<FoundFn>) -> bool {
    let Some((name, rhs)) = assigned_binding(source, bytes, *i) else {
        return false;
    };
    let Some(func) = parse_assigned_fn(source, bytes, &name, *i, rhs) else {
        return false;
    };
    *i = func.body_hi;
    out.push(func);
    true
}

fn assigned_binding(source: &str, bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut j = skip_export_declare(bytes, start);
    let kw_len = binding_kw_len(bytes, j)?;
    j = skip_spaces(bytes, j + kw_len);
    let (name, after_name) = read_ident(source, bytes, j)?;
    j = skip_spaces(bytes, after_name);
    j = skip_optional_type_ann(bytes, j);
    j = skip_spaces(bytes, j);
    eq_rhs(bytes, j).map(|rhs| (name, rhs))
}

fn binding_kw_len(bytes: &[u8], j: usize) -> Option<usize> {
    if is_word(bytes, j, b"const") {
        return Some(5);
    }
    if is_word(bytes, j, b"let") || is_word(bytes, j, b"var") {
        return Some(3);
    }
    None
}

fn eq_rhs(bytes: &[u8], j: usize) -> Option<usize> {
    if bytes.get(j) != Some(&b'=') {
        return None;
    }
    Some(skip_spaces(bytes, j + 1))
}

fn parse_assigned_fn(
    source: &str,
    bytes: &[u8],
    name: &str,
    start: usize,
    mut j: usize,
) -> Option<FoundFn> {
    if is_word(bytes, j, b"async") {
        j = skip_spaces(bytes, j + 5);
    }
    if is_word(bytes, j, b"function") {
        return parse_function_expr(source, bytes, name, start, j);
    }
    parse_arrow_block(source, bytes, name, start, j)
}

fn parse_function_expr(
    source: &str,
    bytes: &[u8],
    name: &str,
    start: usize,
    j: usize,
) -> Option<FoundFn> {
    let after = skip_spaces(bytes, j + 8);
    // Optional name in `function name()`.
    let after = if is_ident_start(bytes.get(after).copied().unwrap_or(0)) {
        read_ident(source, bytes, after)?.1
    } else {
        after
    };
    let after = skip_spaces(bytes, after);
    let after = skip_params_and_return(bytes, after)?;
    let (body_lo, body_hi) = parse_brace_body(bytes, after)?;
    Some(FoundFn {
        name: name.to_owned(),
        start_line: line_of(source, start),
        end_line: line_of(source, body_hi.saturating_sub(1)),
        body_lo,
        body_hi,
    })
}

fn parse_arrow_block(
    source: &str,
    bytes: &[u8],
    name: &str,
    start: usize,
    mut j: usize,
) -> Option<FoundFn> {
    j = skip_arrow_params(bytes, j)?;
    j = skip_spaces(bytes, j);
    j = skip_optional_type_ann(bytes, j);
    j = skip_spaces(bytes, j);
    if !match_op_at(bytes, j, b"=>") {
        return None;
    }
    j = skip_spaces(bytes, j + 2);
    let (body_lo, body_hi) = parse_brace_body(bytes, j)?;
    Some(FoundFn {
        name: name.to_owned(),
        start_line: line_of(source, start),
        end_line: line_of(source, body_hi.saturating_sub(1)),
        body_lo,
        body_hi,
    })
}

fn match_op_at(bytes: &[u8], i: usize, op: &[u8]) -> bool {
    bytes.get(i..i + op.len()) == Some(op)
}

fn parse_function_decl(source: &str, bytes: &[u8], start: usize) -> Option<FoundFn> {
    let j = after_function_keyword(bytes, start);
    let (name, after_name) = read_ident(source, bytes, j)?;
    let after = skip_params_and_return(bytes, skip_spaces(bytes, after_name))?;
    let (body_lo, body_hi) = parse_brace_body(bytes, after)?;
    Some(FoundFn {
        name,
        start_line: line_of(source, start),
        end_line: line_of(source, body_hi.saturating_sub(1)),
        body_lo,
        body_hi,
    })
}

fn after_function_keyword(bytes: &[u8], start: usize) -> usize {
    let mut j = start;
    if is_word(bytes, j, b"async") {
        j = skip_spaces(bytes, j + 5);
    }
    // Caller already matched `function`.
    j = skip_spaces(bytes, j + 8);
    skip_generator_star(bytes, j)
}

fn skip_generator_star(bytes: &[u8], j: usize) -> usize {
    if bytes.get(j) == Some(&b'*') {
        return skip_spaces(bytes, j + 1);
    }
    j
}

pub(super) fn skip_export_declare(bytes: &[u8], mut i: usize) -> usize {
    i = skip_spaces(bytes, i);
    if is_word(bytes, i, b"export") {
        i = skip_spaces(bytes, i + 6);
        if is_word(bytes, i, b"default") {
            i = skip_spaces(bytes, i + 7);
        }
    }
    if is_word(bytes, i, b"declare") {
        i = skip_spaces(bytes, i + 7);
    }
    i
}

pub(super) fn skip_optional_type_ann(bytes: &[u8], j: usize) -> usize {
    let j = skip_spaces(bytes, j);
    if bytes.get(j) != Some(&b':') {
        return j;
    }
    skip_typeish(bytes, skip_spaces(bytes, j + 1))
}

fn skip_arrow_params(bytes: &[u8], j: usize) -> Option<usize> {
    let j = skip_spaces(bytes, j);
    if bytes.get(j) == Some(&b'(') {
        return skip_balanced(bytes, j, b'(', b')');
    }
    if is_ident_start(bytes.get(j).copied().unwrap_or(0)) {
        return read_ident_bytes(bytes, j).map(|(_, next)| next);
    }
    None
}

pub(super) fn skip_params_and_return(bytes: &[u8], mut j: usize) -> Option<usize> {
    j = skip_spaces(bytes, j);
    j = skip_optional_type_params(bytes, j)?;
    if bytes.get(j) != Some(&b'(') {
        return None;
    }
    j = skip_balanced(bytes, j, b'(', b')')?;
    Some(after_return_ann(bytes, j))
}

fn skip_optional_type_params(bytes: &[u8], j: usize) -> Option<usize> {
    if bytes.get(j) != Some(&b'<') {
        return Some(j);
    }
    let j = skip_balanced(bytes, j, b'<', b'>')?;
    Some(skip_spaces(bytes, j))
}

fn after_return_ann(bytes: &[u8], j: usize) -> usize {
    let j = skip_spaces(bytes, j);
    if bytes.get(j) == Some(&b':') {
        return skip_typeish(bytes, skip_spaces(bytes, j + 1));
    }
    j
}

pub(super) fn parse_brace_body(bytes: &[u8], j: usize) -> Option<(usize, usize)> {
    let j = skip_spaces(bytes, j);
    if bytes.get(j) != Some(&b'{') {
        return None;
    }
    let body_hi = skip_balanced(bytes, j, b'{', b'}')?;
    Some((j, body_hi))
}

pub(super) fn skip_typeish(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if matches!(bytes[i], b'{' | b'=' | b';' | b',' | b')' | b'\n') {
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
    let (open, close) = typeish_pair(bytes[i])?;
    skip_balanced(bytes, i, open, close)
}

const fn typeish_pair(b: u8) -> Option<(u8, u8)> {
    if b == b'(' {
        return Some((b'(', b')'));
    }
    if b == b'[' {
        return Some((b'[', b']'));
    }
    if b == b'<' {
        return Some((b'<', b'>'));
    }
    None
}

pub(super) fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

pub(super) fn read_ident(source: &str, bytes: &[u8], i: usize) -> Option<(String, usize)> {
    let (lo, hi) = read_ident_bytes(bytes, i)?;
    Some((source[lo..hi].to_owned(), hi))
}

pub(super) fn read_ident_bytes(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    if !bytes.get(i).is_some_and(|&b| is_ident_start(b)) {
        return None;
    }
    let mut end = i + 1;
    while bytes.get(end).is_some_and(|&b| is_ident_byte(b)) {
        end += 1;
    }
    Some((i, end))
}

pub(super) fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].bytes().filter(|&b| b == b'\n').count() + 1
}

#[cfg(test)]
#[path = "visitor_tests.rs"]
mod tests;
