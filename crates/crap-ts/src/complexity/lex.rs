//! Shared scanners for TypeScript tokens (comments, strings, braces, and words).

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

fn can_start_regex(bytes: &[u8], i: usize) -> bool {
    let Some(prev) = prev_code_byte(bytes, i) else {
        return true;
    };
    if is_regex_prev_punct(prev) {
        return true;
    }
    is_ident_byte(prev) && precedes_regex_keyword(bytes, i)
}

const fn is_regex_prev_punct(b: u8) -> bool {
    matches!(
        b,
        b'(' | b'['
            | b'{'
            | b'='
            | b':'
            | b','
            | b';'
            | b'!'
            | b'&'
            | b'|'
            | b'?'
            | b'+'
            | b'-'
            | b'*'
            | b'%'
            | b'~'
            | b'^'
            | b'\n'
            | b'\r'
    )
}

fn precedes_regex_keyword(bytes: &[u8], i: usize) -> bool {
    let end = prev_non_ws(bytes, i);
    let start = ident_start_before(bytes, end);
    REGEX_KEYWORDS.contains(&&bytes[start..end])
}

const REGEX_KEYWORDS: &[&[u8]] = &[
    b"return",
    b"throw",
    b"case",
    b"else",
    b"typeof",
    b"void",
    b"delete",
    b"new",
    b"await",
    b"yield",
    b"in",
    b"of",
    b"instanceof",
    b"do",
];

fn prev_code_byte(bytes: &[u8], i: usize) -> Option<u8> {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if !bytes[j].is_ascii_whitespace() {
            return Some(bytes[j]);
        }
    }
    None
}

fn skip_regex(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut in_class = false;
    while i < bytes.len() {
        match step_regex(bytes, i, &mut in_class) {
            RegexStep::Continue(next) => i = next,
            RegexStep::Done(end) => return Some(end),
            RegexStep::Fail => return None,
        }
    }
    None
}

enum RegexStep {
    Continue(usize),
    Done(usize),
    Fail,
}

fn step_regex(bytes: &[u8], i: usize, in_class: &mut bool) -> RegexStep {
    if let Some(step) = class_or_escape(bytes, i, in_class) {
        return step;
    }
    end_or_continue(bytes, i, *in_class)
}

fn class_or_escape(bytes: &[u8], i: usize, in_class: &mut bool) -> Option<RegexStep> {
    let b = bytes[i];
    if b == b'\\' {
        return Some(RegexStep::Continue(i.saturating_add(2)));
    }
    if b == b'[' {
        *in_class = true;
        return Some(RegexStep::Continue(i + 1));
    }
    if b == b']' {
        *in_class = false;
        return Some(RegexStep::Continue(i + 1));
    }
    None
}

fn end_or_continue(bytes: &[u8], i: usize, in_class: bool) -> RegexStep {
    let b = bytes[i];
    if b == b'\n' {
        return RegexStep::Fail;
    }
    if is_regex_close(b, in_class) {
        return RegexStep::Done(after_regex_flags(bytes, i + 1));
    }
    RegexStep::Continue(i + 1)
}

const fn is_regex_close(b: u8, in_class: bool) -> bool {
    b == b'/' && !in_class
}

fn after_regex_flags(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b'g' | b'i' | b'm' | b's' | b'u' | b'y' | b'd') {
        i += 1;
    }
    i
}

fn looks_like_jsx_tag(bytes: &[u8], i: usize) -> bool {
    bytes.get(i) == Some(&b'<') && can_start_jsx(bytes, i) && jsx_tag_follows(bytes, i)
}

fn jsx_tag_follows(bytes: &[u8], i: usize) -> bool {
    let Some(&b) = bytes.get(i + 1) else {
        return false;
    };
    jsx_second_byte(bytes, b, i)
}

fn jsx_second_byte(bytes: &[u8], b: u8, i: usize) -> bool {
    if b == b'/' {
        return closing_jsx_name(bytes, i);
    }
    b == b'>' || is_ident_start(b)
}

fn closing_jsx_name(bytes: &[u8], i: usize) -> bool {
    bytes.get(i + 2).is_some_and(|&c| is_ident_start(c))
}

fn can_start_jsx(bytes: &[u8], i: usize) -> bool {
    let Some(prev) = prev_code_byte(bytes, i) else {
        return true;
    };
    if is_jsx_prev_punct(prev) {
        return true;
    }
    is_ident_byte(prev) && precedes_jsx_keyword(bytes, i)
}

const fn is_jsx_prev_punct(b: u8) -> bool {
    matches!(
        b,
        b'(' | b'{'
            | b'['
            | b'='
            | b':'
            | b','
            | b';'
            | b'!'
            | b'?'
            | b'&'
            | b'|'
            | b'>'
            | b'\n'
            | b'\r'
    )
}

fn precedes_jsx_keyword(bytes: &[u8], i: usize) -> bool {
    let end = prev_non_ws(bytes, i);
    let start = ident_start_before(bytes, end);
    JSX_KEYWORDS.contains(&&bytes[start..end])
}

const JSX_KEYWORDS: &[&[u8]] = &[
    b"return", b"throw", b"else", b"typeof", b"await", b"yield", b"case", b"default", b"do", b"of",
    b"in", b"void", b"new", b"delete",
];

fn prev_non_ws(bytes: &[u8], i: usize) -> usize {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if !bytes[j].is_ascii_whitespace() {
            return j + 1;
        }
    }
    0
}

fn ident_start_before(bytes: &[u8], end: usize) -> usize {
    let mut j = end;
    while j > 0 && is_ident_byte(bytes[j - 1]) {
        j -= 1;
    }
    j
}

fn skip_jsx_tag(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut in_quote: Option<u8> = None;
    while i < bytes.len() {
        match step_jsx(bytes, i, &mut in_quote) {
            JsxOut::Done(end) => return Some(end),
            JsxOut::Next(next) => i = next,
            JsxOut::Fail => return None,
        }
    }
    None
}

enum JsxOut {
    Done(usize),
    Next(usize),
    Fail,
}

fn step_jsx(bytes: &[u8], i: usize, in_quote: &mut Option<u8>) -> JsxOut {
    if let Some(q) = *in_quote {
        return JsxOut::Next(after_jsx_quote_byte(bytes, i, q, in_quote));
    }
    unquoted_jsx_byte(bytes, i, in_quote)
}

fn unquoted_jsx_byte(bytes: &[u8], i: usize, in_quote: &mut Option<u8>) -> JsxOut {
    let b = bytes[i];
    if is_jsx_attr_quote(b) {
        *in_quote = Some(b);
        return JsxOut::Next(i + 1);
    }
    if b == b'{' {
        return balanced_jsx_or_fail(bytes, i);
    }
    if b == b'>' {
        return JsxOut::Done(i + 1);
    }
    JsxOut::Next(i + 1)
}

const fn is_jsx_attr_quote(b: u8) -> bool {
    b == b'"' || b == b'\''
}

fn balanced_jsx_or_fail(bytes: &[u8], i: usize) -> JsxOut {
    skip_balanced(bytes, i, b'{', b'}').map_or(JsxOut::Fail, JsxOut::Next)
}

fn after_jsx_quote_byte(bytes: &[u8], i: usize, quote: u8, in_quote: &mut Option<u8>) -> usize {
    if bytes[i] == quote {
        *in_quote = None;
        return i + 1;
    }
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

/// Returns whether `b` can continue a TypeScript identifier.
pub(super) const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Returns whether `b` can start a TypeScript identifier.
pub(super) const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

#[cfg(test)]
#[path = "lex_tests.rs"]
mod tests;
