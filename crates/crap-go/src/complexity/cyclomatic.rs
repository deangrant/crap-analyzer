//! Cyclomatic complexity for a Go function body.

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
    if match_op(bytes, i, b"&&") || match_op(bytes, i, b"||") {
        *count += 1;
        return Some(i + 2);
    }
    for word in [b"if" as &[u8], b"for", b"case", b"default"] {
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
        let parsed = analyze_source(Path::new("t.go"), src, Metric::Cyclomatic);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default().first().map_or(0, |f| f.complexity)
    }

    #[test]
    fn straight_line_is_one() {
        assert_eq!(snippet("package p\nfunc trivial() { x := 1 }\n"), 1);
    }

    #[test]
    fn if_adds_one() {
        assert_eq!(snippet("package p\nfunc f(x int) { if x > 0 { x } }\n"), 2);
    }

    #[test]
    fn bool_ops_and_case_add() {
        let src = "package p\nfunc f(x int) {\n  if x > 0 && x < 10 || x == 0 {\n\
            switch x {\n    case 1:\n    }\n  }\n}\n";
        // Base + if + && + || + case; switch itself is not a decision.
        assert_eq!(snippet(src), 5);
    }

    #[test]
    fn switch_case_is_two() {
        assert_eq!(
            snippet("package p\nfunc f(x int) { switch x { case 1: } }\n"),
            2
        );
    }

    #[test]
    fn switch_cases_and_default() {
        let src =
            "package p\nfunc f(x int) {\n  switch x {\n  case 1:\n  case 2:\n  default:\n  }\n}\n";
        assert_eq!(snippet(src), 4);
    }

    #[test]
    fn select_default_is_two() {
        assert_eq!(snippet("package p\nfunc f() { select { default: } }\n"), 2);
    }

    #[test]
    fn else_if_chain_counts_each_if() {
        let src =
            "package p\nfunc f(x int) {\n  if x > 0 {\n  } else if x < 0 {\n  } else {\n  }\n}\n";
        // Base + if + if; else does not add.
        assert_eq!(snippet(src), 3);
    }

    #[test]
    fn type_switch_counts_cases_not_switch() {
        let src = "package p\nfunc f(x any) {\n  switch x.(type) {\n  case int:\n\
            case string:\n  default:\n  }\n}\n";
        // Base + case + case + default.
        assert_eq!(snippet(src), 4);
    }

    #[test]
    fn comments_and_strings_are_ignored() {
        let src = r#"package p
func f() {
  // if ignored
  /* for ignored */
  x := "if" + `for` + 'c'
  if true { x = "a\"b" }
}
"#;
        assert_eq!(snippet(src), 2);
    }

    #[test]
    fn trailing_comment_body() {
        assert_eq!(snippet("package p\nfunc f() { x := 1 // done\n}\n"), 1);
    }

    #[test]
    fn comment_only_tail_after_decision() {
        assert_eq!(snippet("package p\nfunc f() { if x //\n}\n"), 2);
    }

    #[test]
    fn unterminated_string_in_body() {
        assert_eq!(super::count("{ x := \"abc"), 1);
    }

    #[test]
    fn skip_noise_eof() {
        assert_eq!(super::count("// only"), 1);
    }
}
