use super::skip_file;

#[test]
fn no_constraint_is_kept() {
    assert!(!skip_file("package main\n", &[]));
}

#[test]
fn unknown_tag_skips_file() {
    assert!(skip_file("//go:build fancy\n\npackage main\n", &[]));
}

#[test]
fn enabled_tag_keeps_file() {
    assert!(!skip_file(
        "//go:build fancy\n\npackage main\n",
        &["fancy".into()]
    ));
}

#[test]
fn not_and_or_parens() {
    let src = "//go:build !(windows && fancy) || linux\n\npackage main\n";
    // On linux host, linux is true so overall true → do not skip.
    if cfg!(target_os = "linux") {
        assert!(!skip_file(src, &[]));
    }
    assert!(skip_file(
        "//go:build windows && fancy\n\npackage main\n",
        &[]
    ));
}

#[test]
fn plus_build_lines() {
    assert!(skip_file("// +build fancy\n\npackage main\n", &[]));
    assert!(!skip_file(
        "// +build fancy\n\npackage main\n",
        &["fancy".into()]
    ));
}

#[test]
fn ordinary_comment_before_package_is_ignored() {
    let src = "//go:build fancy\n// copyright\n\npackage main\n";
    assert!(skip_file(src, &[]));
    assert!(!skip_file(src, &["fancy".into()]));
}

#[test]
fn plus_build_alternatives_on_one_line() {
    let src = "// +build fancy integration\n\npackage main\n";
    assert!(skip_file(src, &[]));
    assert!(!skip_file(src, &["integration".into()]));
}

#[test]
fn unmatched_paren_treats_constraint_as_false() {
    assert!(skip_file(
        "//go:build (fancy\n\npackage main\n",
        &["fancy".into()]
    ));
}

#[test]
fn dangling_operator_tokens_are_false() {
    assert!(skip_file("//go:build &&\n\npackage main\n", &[]));
}

#[test]
fn unknown_chars_in_constraint_are_skipped() {
    assert!(skip_file("//go:build fancy@home\n\npackage main\n", &[]));
    assert!(!skip_file(
        "//go:build fancy@home\n\npackage main\n",
        &["fancy".into()]
    ));
}

#[test]
fn plus_build_negated_tag() {
    assert!(!skip_file("// +build !fancy\n\npackage main\n", &[]));
    assert!(skip_file(
        "// +build !fancy\n\npackage main\n",
        &["fancy".into()]
    ));
}

#[test]
fn host_os_tags_are_evaluated() {
    assert!(skip_file("//go:build darwin\n\npackage main\n", &[]));
    assert!(skip_file("//go:build windows\n\npackage main\n", &[]));
    if cfg!(target_os = "linux") {
        assert!(!skip_file("//go:build linux\n\npackage main\n", &[]));
        assert!(!skip_file("//go:build unix\n\npackage main\n", &[]));
    }
}
