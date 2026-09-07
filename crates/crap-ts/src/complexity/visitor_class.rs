//! Class declarations and method body attribution.

use super::{
    FoundFn, line_of, parse_brace_body, read_ident, read_ident_bytes, skip_export_declare,
    skip_params_and_return, skip_spaces, skip_typeish,
};
use crate::complexity::lex::{is_word, skip_balanced, skip_noise};

pub(super) fn try_take_class(
    source: &str,
    bytes: &[u8],
    i: &mut usize,
    out: &mut Vec<FoundFn>,
) -> bool {
    let mut j = skip_export_declare(bytes, *i);
    if !is_word(bytes, j, b"class") {
        return false;
    }
    j = skip_spaces(bytes, j + 5);
    let Some((class_name, after_name)) = read_ident(source, bytes, j) else {
        *i += 1;
        return true;
    };
    j = skip_spaces(bytes, after_name);
    j = skip_heritage(bytes, j);
    j = skip_spaces(bytes, j);
    let Some(body_hi) = skip_balanced(bytes, j, b'{', b'}') else {
        *i += 1;
        return true;
    };
    collect_class_methods(source, bytes, &class_name, j + 1, body_hi - 1, out);
    *i = body_hi;
    true
}

fn collect_class_methods(
    source: &str,
    bytes: &[u8],
    class_name: &str,
    mut i: usize,
    end: usize,
    out: &mut Vec<FoundFn>,
) {
    while i < end {
        i = skip_noise(bytes, i);
        if i >= end {
            break;
        }
        i = skip_decorators(bytes, i);
        i = skip_spaces(bytes, i);
        if let Some(func) = try_parse_method(source, bytes, class_name, i, end) {
            out.push(func.clone());
            i = func.body_hi;
            continue;
        }
        i += 1;
    }
}

fn try_parse_method(
    source: &str,
    bytes: &[u8],
    class_name: &str,
    start: usize,
    end: usize,
) -> Option<FoundFn> {
    let mut j = skip_method_modifiers(bytes, start);
    let (method_name, after_name) = read_method_name(source, bytes, j)?;
    j = skip_spaces(bytes, after_name);
    let after = method_after_params(bytes, j, end)?;
    let (body_lo, body_hi) = parse_brace_body(bytes, after)?;
    Some(FoundFn {
        name: format!("{class_name}.{method_name}"),
        start_line: line_of(source, start),
        end_line: line_of(source, body_hi.saturating_sub(1)),
        body_lo,
        body_hi,
    })
}

fn method_after_params(bytes: &[u8], j: usize, end: usize) -> Option<usize> {
    if bytes.get(j) != Some(&b'(') {
        return None;
    }
    let j = skip_params_and_return(bytes, j)?;
    (j < end).then_some(j)
}

fn read_method_name(source: &str, bytes: &[u8], j: usize) -> Option<(String, usize)> {
    if let Some(named) = accessor_method_name(source, bytes, j) {
        return Some(named);
    }
    if bytes.get(j) == Some(&b'[') {
        let close = skip_balanced(bytes, j, b'[', b']')?;
        return Some((source[j..close].to_owned(), close));
    }
    read_ident(source, bytes, j)
}

fn accessor_method_name(source: &str, bytes: &[u8], j: usize) -> Option<(String, usize)> {
    let kind = if is_word(bytes, j, b"get") {
        "get"
    } else if is_word(bytes, j, b"set") {
        "set"
    } else {
        return None;
    };
    let after = skip_spaces(bytes, j + kind.len());
    let (name, next) = read_ident(source, bytes, after)?;
    Some((format!("{kind} {name}"), next))
}

fn skip_method_modifiers(bytes: &[u8], mut j: usize) -> usize {
    loop {
        j = skip_spaces(bytes, j);
        let advanced = [
            (b"public" as &[u8], 6_usize),
            (b"private", 7),
            (b"protected", 9),
            (b"static", 6),
            (b"async", 5),
            (b"readonly", 8),
            (b"abstract", 8),
            (b"override", 8),
        ]
        .into_iter()
        .find_map(|(word, len)| is_word(bytes, j, word).then_some(len));
        let Some(len) = advanced else {
            return j;
        };
        j += len;
    }
}

fn skip_decorators(bytes: &[u8], mut i: usize) -> usize {
    loop {
        i = skip_spaces(bytes, i);
        if bytes.get(i) != Some(&b'@') {
            return i;
        }
        i = after_decorator(bytes, i + 1);
    }
}

fn after_decorator(bytes: &[u8], mut i: usize) -> usize {
    if let Some((_, next)) = read_ident_bytes(bytes, i) {
        i = next;
    }
    i = skip_spaces(bytes, i);
    if bytes.get(i) == Some(&b'(') {
        return skip_balanced(bytes, i, b'(', b')').unwrap_or(i + 1);
    }
    i
}

fn skip_heritage(bytes: &[u8], mut j: usize) -> usize {
    for word in [b"extends" as &[u8], b"implements"] {
        j = skip_spaces(bytes, j);
        if is_word(bytes, j, word) {
            j = skip_spaces(bytes, j + word.len());
            j = skip_typeish(bytes, j);
        }
    }
    j
}
