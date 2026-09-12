// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use crate::complexity::analyze_source;
use crap_core::Metric;
use std::path::Path;

fn names(src: &str) -> Vec<String> {
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
    assert!(parsed.is_ok(), "{parsed:?}");
    parsed.unwrap_or_default().into_iter().map(|f| f.name).collect()
}

#[test]
fn finds_module_and_method() {
    let src = "def top():\n    pass\n\nclass Box:\n    def method(self):\n        pass\n";
    assert_eq!(names(src), vec!["top".to_owned(), "Box.method".to_owned()]);
}

#[test]
fn class_stack_pops_when_indent_leaves() {
    let src = concat!(
        "class Outer:\n",
        "    class Inner:\n",
        "        def inner_m(self):\n",
        "            pass\n",
        "    def outer_m(self):\n",
        "        pass\n",
        "class Sibling:\n",
        "    def sib(self):\n",
        "        pass\n",
    );
    assert_eq!(
        names(src),
        vec![
            "Inner.inner_m".to_owned(),
            "Outer.outer_m".to_owned(),
            "Sibling.sib".to_owned(),
        ]
    );
}

#[test]
fn nested_def_is_separate_row() {
    let src = "def outer():\n    def inner():\n        return 1\n    return inner()\n";
    assert_eq!(names(src), vec!["outer".to_owned(), "inner".to_owned()]);
}

#[test]
fn async_def_and_oneliner() {
    let src = "async def ago():\n    pass\n\ndef one(): return 1\n";
    assert_eq!(names(src), vec!["ago".to_owned(), "one".to_owned()]);
}

#[test]
fn decorator_does_not_block_def() {
    let src = "@dec\ndef decorated():\n    pass\n";
    assert_eq!(names(src), vec!["decorated".to_owned()]);
}

#[test]
fn lambda_is_not_a_row() {
    let src = "def f():\n    g = lambda x: x\n    return g(1)\n";
    assert_eq!(names(src), vec!["f".to_owned()]);
}

#[test]
fn structure_failure_is_collect_error() {
    let err = analyze_source(Path::new("bad.py"), "def f(:\n", Metric::Cyclomatic);
    assert!(err.is_err());
}

#[test]
fn span_covers_suite_lines() {
    let src = "def f():\n    x = 1\n    return x\n";
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
    assert!(parsed.is_ok(), "{parsed:?}");
    let f = &parsed.unwrap_or_default()[0];
    assert_eq!(f.start_line, 1);
    assert_eq!(f.end_line, 3);
}

#[test]
fn class_without_name_is_ignored() {
    let src = "class :\n    pass\n";
    assert!(names(src).is_empty());
}

#[test]
fn def_without_name_or_colon_is_ignored() {
    assert!(names("def \n").is_empty());
    assert!(names("def foo\n").is_empty());
    assert!(names("def foo").is_empty());
    assert!(names("def foo #").is_empty());
}

#[test]
fn find_def_colon_stray_closer_at_depth_zero() {
    assert_eq!(super::find_def_colon(b"]:", 0), Some(1));
}

#[test]
fn line_index_at_past_eof_uses_last_line() {
    let lines = super::split_lines(b"def f():\n    pass\n");
    let last = lines.len() - 1;
    assert_eq!(super::line_index_at(&lines, usize::MAX), last);
}

#[test]
fn typed_params_and_brackets_in_signature() {
    let src = "def f(x: list[int]) -> None:\n    pass\n";
    assert_eq!(names(src), vec!["f".to_owned()]);
}

#[test]
fn type_params_brackets_in_signature() {
    let src = "def f[T](x: T) -> T:\n    pass\n";
    assert_eq!(names(src), vec!["f".to_owned()]);
}

#[test]
fn multiline_signature_emits_row_and_suite_span() {
    let src = concat!("def foo(\n", "    a,\n", ") -> None:\n", "    pass\n",);
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
    assert!(parsed.is_ok(), "{parsed:?}");
    let f = &parsed.unwrap_or_default()[0];
    assert_eq!(f.name, "foo");
    assert_eq!(f.start_line, 1);
    assert_eq!(f.end_line, 4);
}

#[test]
fn multiline_async_method_under_class() {
    let src = concat!(
        "class Box:\n",
        "    async def method(\n",
        "        self,\n",
        "        x: int,\n",
        "    ) -> int:\n",
        "        return x\n",
    );
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
    assert!(parsed.is_ok(), "{parsed:?}");
    let f = &parsed.unwrap_or_default()[0];
    assert_eq!(f.name, "Box.method");
    assert_eq!(f.start_line, 2);
    assert_eq!(f.end_line, 6);
}

#[test]
fn multiline_type_params_in_signature() {
    let src = concat!("def f[T](\n", "    x: T,\n", ") -> T:\n", "    return x\n");
    assert_eq!(names(src), vec!["f".to_owned()]);
}

#[test]
fn tabs_count_as_indent() {
    let src = "def f():\n\treturn 1\n";
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
    assert!(parsed.is_ok(), "{parsed:?}");
    assert_eq!(parsed.unwrap_or_default()[0].end_line, 2);
}

#[test]
fn blank_lines_inside_suite_keep_body() {
    let src = "def f():\n    x = 1\n\n    return x\n";
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
    assert!(parsed.is_ok(), "{parsed:?}");
    assert_eq!(parsed.unwrap_or_default()[0].end_line, 4);
}

#[test]
fn nested_masking_does_not_double_count() {
    let src = concat!(
        "def outer():\n",
        "    def inner():\n",
        "        if True:\n",
        "            return 1\n",
        "    return inner()\n",
    );
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cyclomatic);
    assert!(parsed.is_ok(), "{parsed:?}");
    let fns = parsed.unwrap_or_default();
    let outer = fns.iter().find(|f| f.name == "outer");
    assert!(outer.is_some());
    assert_eq!(outer.map(|f| f.complexity), Some(1));
}
