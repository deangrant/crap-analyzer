//! Cyclomatic complexity for a TypeScript function body.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::lex::{is_word, match_op, skip_noise};

/// Returns cyclomatic complexity for `body` (minimum 1).
pub(super) fn count(body: &str) -> usize {
    let mut count = 1_usize;
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        i = skip_noise(bytes, i);
        if i >= bytes.len() {
            break;
        }
        if let Some(next) = take_decision(bytes, i, &mut count) {
            i = next;
            continue;
        }
        i += 1;
    }
    count
}

fn take_decision(bytes: &[u8], i: usize, count: &mut usize) -> Option<usize> {
    take_bool_op(bytes, i, count)
        .or_else(|| take_ternary(bytes, i, count))
        .or_else(|| take_keyword(bytes, i, count))
}

fn take_bool_op(bytes: &[u8], i: usize, count: &mut usize) -> Option<usize> {
    if match_op(bytes, i, b"??") || match_op(bytes, i, b"&&") || match_op(bytes, i, b"||") {
        *count += 1;
        return Some(i + 2);
    }
    None
}

fn take_ternary(bytes: &[u8], i: usize, count: &mut usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'?') {
        return None;
    }
    if matches!(bytes.get(i + 1), Some(&b'?' | &b'.')) {
        return None;
    }
    *count += 1;
    Some(i + 1)
}

fn take_keyword(bytes: &[u8], i: usize, count: &mut usize) -> Option<usize> {
    for word in [b"if" as &[u8], b"for", b"while", b"case", b"catch", b"do"] {
        if is_word(bytes, i, word) {
            *count += 1;
            return Some(i + word.len());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::complexity::analyze_source;
    use crap_core::Metric;
    use std::path::Path;

    fn snippet(src: &str) -> usize {
        let parsed = analyze_source(Path::new("t.ts"), src, Metric::Cyclomatic);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default().first().map_or(0, |f| f.complexity)
    }

    #[test]
    fn straight_line_is_one() {
        assert_eq!(snippet("function trivial() { const x = 1; }\n"), 1);
    }

    #[test]
    fn if_adds_one() {
        assert_eq!(snippet("function f(x: number) { if (x > 0) { x; } }\n"), 2);
    }

    #[test]
    fn bool_ops_and_case_add() {
        let src = "function f(x: number) {\n  if (x > 0 && x < 10 || x === 0) {\n\
            switch (x) {\n    case 1:\n    }\n  }\n}\n";
        // Base + if + && + || + case; switch itself is not a decision.
        assert_eq!(snippet(src), 5);
    }

    #[test]
    fn nullish_and_ternary_add() {
        let src = "function f(x: number | null) { return x ?? (x ? 1 : 0); }\n";
        // Base + ?? + ?
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn else_if_chain_counts_each_if() {
        let src =
            "function f(x: number) {\n  if (x > 0) {\n  } else if (x < 0) {\n  } else {\n  }\n}\n";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn comments_and_strings_are_ignored() {
        let src = r#"function f() {
  // if ignored
  /* for ignored */
  const x = "if" + `for` + 'c';
  if (true) { return /if/; }
}
"#;
        assert_eq!(snippet(src), 2);
    }

    #[test]
    fn while_and_do_add() {
        assert_eq!(
            snippet("function f() { while (true) { break; } do { break; } while (false); }\n"),
            4
        );
    }

    #[test]
    fn optional_chaining_is_not_ternary() {
        assert_eq!(
            snippet("function f(x: { a?: number }) { return x?.a; }\n"),
            1
        );
    }

    #[test]
    fn trailing_comment_reaches_eof_after_noise() {
        assert_eq!(super::count("{ // only comment"), 1);
    }
}
