//! Cyclomatic complexity for a Python function body.

use super::lex::{is_word, skip_noise};

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
    take_bool_op(bytes, i, count).or_else(|| take_keyword(bytes, i, count))
}

fn take_bool_op(bytes: &[u8], i: usize, count: &mut usize) -> Option<usize> {
    if is_word(bytes, i, b"and") {
        *count += 1;
        return Some(i + 3);
    }
    if is_word(bytes, i, b"or") {
        *count += 1;
        return Some(i + 2);
    }
    None
}

fn take_keyword(bytes: &[u8], i: usize, count: &mut usize) -> Option<usize> {
    for word in [
        b"elif" as &[u8],
        b"except",
        b"while",
        b"case",
        b"for",
        b"if",
    ] {
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
        let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default().first().map_or(0, |f| f.complexity)
    }

    #[test]
    fn straight_line_is_one() {
        assert_eq!(snippet("def trivial():\n    x = 1\n"), 1);
    }

    #[test]
    fn if_adds_one() {
        assert_eq!(snippet("def f(x):\n    if x > 0:\n        return x\n"), 2);
    }

    #[test]
    fn bool_ops_and_elif_add() {
        let src = concat!(
            "def f(x):\n",
            "    if x > 0 and x < 10 or x == 0:\n",
            "        pass\n",
            "    elif x < 0:\n",
            "        pass\n",
        );
        // Base + if + and + or + elif
        assert_eq!(snippet(src), 5);
    }

    #[test]
    fn ternary_if_adds() {
        let src = "def f(x):\n    return 1 if x else 0\n";
        assert_eq!(snippet(src), 2);
    }

    #[test]
    fn comments_and_strings_are_ignored() {
        let src =
            "def f():\n    # if ignored\n    x = \"if\" + 'for'\n    if True:\n        return 1\n";
        assert_eq!(snippet(src), 2);
    }

    #[test]
    fn while_and_for_add() {
        let src =
            "def f():\n    while True:\n        break\n    for _ in range(1):\n        break\n";
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn except_and_case_add() {
        let src = concat!(
            "def f(x):\n",
            "    try:\n",
            "        pass\n",
            "    except ValueError:\n",
            "        pass\n",
            "    match x:\n",
            "        case 1:\n",
            "            pass\n",
        );
        // Base + except + case; try/match themselves are not decisions.
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn trailing_comment_reaches_eof_after_noise() {
        assert_eq!(super::count("# only comment"), 1);
    }
}
