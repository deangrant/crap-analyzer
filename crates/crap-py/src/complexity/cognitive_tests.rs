use crate::complexity::analyze_source;
use crap_core::Metric;
use std::path::Path;

fn snippet(src: &str) -> usize {
    let parsed = analyze_source(Path::new("t.py"), src, Metric::Cognitive);
    assert!(parsed.is_ok(), "{parsed:?}");
    parsed.unwrap_or_default().first().map_or(0, |f| f.complexity)
}

#[test]
fn nested_if_weights_inner() {
    let src = "def f(x):\n    if x > 0:\n        if x > 1:\n            return x\n";
    assert_eq!(snippet(src), 3);
}

#[test]
fn else_is_flat() {
    let src = "def f(x):\n    if x:\n        return 1\n    else:\n        return 0\n";
    assert_eq!(snippet(src), 2);
}

#[test]
fn and_or_run_counts_once() {
    let src = "def f(x):\n    if x and x and x:\n        return 1\n";
    assert_eq!(snippet(src), 2);
}

#[test]
fn for_and_while_nest() {
    let src = "def f(xs):\n    for x in xs:\n        while x:\n            break\n";
    // for (1) + while (1+1=2) = 3
    assert_eq!(snippet(src), 3);
}

#[test]
fn except_is_flat() {
    let src = "def f():\n    try:\n        pass\n    except ValueError:\n        pass\n";
    // try (+1) + except flat (+1) = 2
    assert_eq!(snippet(src), 2);
}

#[test]
fn or_run_counts_once() {
    let src = "def f(x):\n    if x or x or x:\n        return 1\n";
    assert_eq!(snippet(src), 2);
}

#[test]
fn with_and_match_nest() {
    let src = concat!(
        "def f(x):\n",
        "    with open('f') as fh:\n",
        "        match x:\n",
        "            case 1:\n",
        "                return fh\n",
    );
    // with(1) + match(2) = 3
    assert_eq!(snippet(src), 3);
}

#[test]
fn blank_and_comment_lines_are_skipped() {
    let src = "def f():\n    # comment\n\n    return 1\n";
    assert_eq!(snippet(src), 0);
}

#[test]
fn bool_run_at_eof_and_trailing_comment() {
    assert_eq!(super::count("return x and"), 1);
    assert_eq!(super::count("return x and y #"), 1);
    // Trailing comment with no bool-op must still advance past noise.
    assert_eq!(super::count("return x #"), 0);
}
