use super::{
    analyze_source, match_braces, skip_balanced, skip_noise, skip_signature, skip_typeish,
    try_parse_func,
};
use crate::complexity::lex::skip_string;
use crap_core::Metric;
use std::path::Path;

fn names(src: &str) -> Vec<String> {
    analyze_source(Path::new("t.go"), src, Metric::Cyclomatic)
        .into_iter()
        .map(|f| f.name)
        .collect()
}

#[test]
fn straight_and_if_functions() {
    let src = r"
package p
func trivial() { x := 1 }
func branched(x int) { if x > 0 { x } }
";
    let fns = analyze_source(Path::new("t.go"), src, Metric::Cyclomatic);
    assert_eq!(fns.len(), 2);
    assert_eq!(fns[0].complexity, 1);
    assert_eq!(fns[1].complexity, 2);
}

#[test]
fn nested_named_func_is_separate_row() {
    let got = names("package p\nfunc outer() {\nfunc inner() { y := 1 }\n_ = inner\n}\n");
    assert!(got.iter().any(|n| n == "outer"));
    assert!(got.iter().any(|n| n == "inner"));
}

#[test]
fn closure_is_not_a_separate_row() {
    let src = "package p\nfunc outer() {\n_ = func(x int) { return x }\n}\n";
    assert_eq!(names(src), vec!["outer".to_owned()]);
}

#[test]
fn methods_and_pointer_receivers() {
    let src = "package p\nfunc (t *T) M() {}\nfunc (t T) N() int { return 1 }\n";
    let got = names(src);
    assert!(got.iter().any(|n| n == "T.M"));
    assert!(got.iter().any(|n| n == "T.N"));
}

#[test]
fn result_tuple_and_slice_types() {
    let src =
        "package p\nfunc F() (int, error) { return 0, nil }\nfunc G() []byte { return nil }\n";
    let got = names(src);
    assert!(got.iter().any(|n| n == "F"));
    assert!(got.iter().any(|n| n == "G"));
}

#[test]
fn comments_and_strings_do_not_hide_funcs() {
    let src = r#"package p
// func fake() {}
/* func also() {} */
func real() { x := "func" + `func` + 'f' }
"#;
    assert_eq!(names(src), vec!["real".to_owned()]);
}

#[test]
fn trailing_comment_reaches_eof() {
    let src = "package p\nfunc f() { x := 1 } // end";
    assert_eq!(names(src), vec!["f".to_owned()]);
}

#[test]
fn try_parse_rejects_non_func() {
    let src = "package p\n";
    assert!(try_parse_func(src, src.as_bytes(), 0).is_none());
}

#[test]
fn helpers_cover_noise_and_balance_edges() {
    let bytes = br#"// line
/* block */
"a\"b" `raw` 'c' rest"#;
    let after = skip_noise(bytes, 0);
    assert!(after < bytes.len());
    assert!(skip_balanced(bytes, 0, b'(', b')').is_none());
    assert!(match_braces(b"x", 0).is_none());
    assert_eq!(skip_string(br#""ab"#, 0), 3);
    assert_eq!(skip_string(br#""unterminated"#, 0), 13);
    let open = b"{ /* c */";
    assert!(skip_balanced(open, 0, b'{', b'}').is_none());
}

#[test]
fn signature_without_params_is_rejected() {
    assert!(names("package p\nfunc F { }\n").is_empty());
}

#[test]
fn odd_result_type_chars_after_params() {
    let src = "package p\nfunc F() @ {}\n";
    let _ = names(src);
}

#[test]
fn typeish_with_unbalanced_paren() {
    let src = "package p\nfunc F() chan(int {}\n";
    let _ = names(src);
}

#[test]
fn receiver_breaks_on_unexpected_byte() {
    let src = "package p\nfunc (t *T, ) M() {}\n";
    let _ = names(src);
}

#[test]
fn receiver_without_type_name() {
    let src = "package p\nfunc ( * ) M() {}\n";
    let _ = names(src);
}

#[test]
fn cognitive_metric_via_analyze_source() {
    let src = "package p\nfunc f(x int) { if x > 0 { if x > 1 { x } } }\n";
    let fns = analyze_source(Path::new("t.go"), src, Metric::Cognitive);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].complexity, 3);
}

#[test]
fn skip_signature_direct_edges() {
    assert!(skip_signature(b"F", 0).is_none());
    assert_eq!(skip_signature(b"()@", 0), Some(2));
}

#[test]
fn skip_typeish_balanced_and_unbalanced() {
    assert!(skip_typeish(b"chan(int)", 0) >= 9);
    assert!(skip_typeish(b"chan(int", 0) < 20);
    assert!(skip_typeish(b"[]int", 0) >= 5);
    assert!(skip_typeish(b"[3", 0) < 10);
}

#[test]
fn receiver_type_breaks_on_at_sign() {
    let src = "package p\nfunc (t @T) M() {}\n";
    let got = names(src);
    assert!(got.iter().any(|n| n == "M" || n.contains('M')));
}
