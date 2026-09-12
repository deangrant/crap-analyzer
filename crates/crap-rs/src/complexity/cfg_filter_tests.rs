use super::super::analyze_source_cfg;
use super::CfgUniverse;
use crap_core::{FunctionComplexity, Metric};
use std::path::Path;

fn cyclo(src: &str) -> Vec<FunctionComplexity> {
    cyclo_features(src, &CfgUniverse::new(std::iter::empty::<String>()))
}

fn cyclo_features(src: &str, cfg: &CfgUniverse) -> Vec<FunctionComplexity> {
    let parsed = analyze_source_cfg(Path::new("t.rs"), src, Metric::Cyclomatic, cfg);
    assert!(parsed.is_ok(), "{parsed:?}");
    parsed.unwrap_or_default()
}

fn assert_only_keep(src: &str) {
    let fns = cyclo(src);
    assert_eq!(fns.len(), 1, "{src}");
    assert_eq!(fns[0].name, "keep", "{src}");
}

fn assert_none(src: &str) {
    assert!(cyclo(src).is_empty(), "{src}");
}

fn assert_single_named(src: &str, name: &str) {
    let fns = cyclo(src);
    assert_eq!(fns.len(), 1, "{src}");
    assert_eq!(fns[0].name, name, "{src}");
}

fn assert_host_cfg_kept(cfg_key: &str, host_value: &str) {
    assert!(!host_value.is_empty(), "{cfg_key}");
    let src = format!("#[cfg({cfg_key} = \"{host_value}\")] fn f() {{}}");
    assert_single_named(&src, "f");
}

fn assert_host_cfg_kept_allow_empty(cfg_key: &str, host_value: &str) {
    let src = format!("#[cfg({cfg_key} = \"{host_value}\")] fn f() {{}}");
    assert_single_named(&src, "f");
}

fn assert_gated_or_keep(fns: &[FunctionComplexity], gated_present: bool) {
    if gated_present {
        assert_eq!(fns.len(), 2);
    } else {
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }
}

#[test]
fn skip_attrs_and_modules_leave_keep() {
    for src in [
        "#[test] fn t() { if true {} } fn keep() {}",
        "#[bench] fn b() { if true {} } fn keep() {}",
        "#[tokio::test] fn t() { if true {} } fn keep() {}",
        "mod tests { fn helper() { if true {} } } fn keep() {}",
        "mod test { fn helper() { if true {} } } fn keep() {}",
        "#[cfg(test)] fn helper() { if true {} } fn keep() {}",
        "#[cfg(all(test, feature = \"x\"))] fn helper() { if true {} } fn keep() {}",
        "#[cfg(test)] mod tests { fn helper() { if true {} } } fn keep() {}",
        "#[cfg(any(test))] fn helper() { if true {} } fn keep() {}",
        "#[cfg(feature = \"x\")] fn f() { if true {} } fn keep() {}",
        "#[cfg(feature = \"off\")] fn gated() {} fn keep() {}",
        "#[cfg(foo)] fn f() {} fn keep() {}",
        "#[cfg(proc_macro)] fn f() {} fn keep() {}",
    ] {
        assert_only_keep(src);
    }
}

#[test]
fn other_module_helpers_are_still_scored() {
    let fns = cyclo("mod helpers { fn helper() { if true {} } } fn keep() {}");
    assert_eq!(fns.len(), 2);
    let names: Vec<_> = fns.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"keep"));
}

#[test]
fn cfg_any_test_or_unix_is_kept() {
    let fns = cyclo("#[cfg(any(test, unix))] fn f() { if true {} }");
    #[cfg(unix)]
    {
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }
    #[cfg(not(unix))]
    assert!(fns.is_empty());
}

#[test]
fn empty_cfg_skips() {
    for src in [
        "struct Foo; #[cfg(test)] impl Foo { fn bar(&self) { if true {} } }",
        "#[cfg(test)] trait T { fn m(&self) { if true {} } }",
        "#[cfg(target_os = \"unknown-os\")] fn f() {}",
        "#[cfg(target_arch = \"unknown-arch\")] fn f() {}",
    ] {
        assert_none(src);
    }
}

#[test]
fn cfg_feature_is_kept_when_enabled() {
    let cfg = CfgUniverse::new(["x"]);
    let fns = cyclo_features("#[cfg(feature = \"x\")] fn f() { if true {} }", &cfg);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_not_feature_is_kept_when_disabled() {
    assert_single_named("#[cfg(not(feature = \"x\"))] fn f() { if true {} }", "f");
}

#[test]
fn cfg_unknown_list_and_key_are_skipped() {
    assert_none("#[cfg(weird(x))] fn f() {}");
    assert_none("#[cfg(foo = \"bar\")] fn f() {}");
    assert_none("#[cfg(foo = 1)] fn f() {}");
}

#[test]
fn host_target_cfgs_are_kept() {
    for (key, value) in [
        ("target_os", super::HOST_OS),
        ("target_arch", super::HOST_ARCH),
        ("target_family", super::HOST_FAMILY),
        ("target_pointer_width", super::HOST_POINTER_WIDTH),
        ("target_endian", super::HOST_ENDIAN),
    ] {
        assert_host_cfg_kept(key, value);
    }
    assert_host_cfg_kept_allow_empty("target_env", super::HOST_ENV);
    assert_host_cfg_kept_allow_empty("target_vendor", super::HOST_VENDOR);
}

#[test]
fn host_flag_cfgs_are_gated() {
    for (src, gated_present) in [
        (
            "#[cfg(target_os = \"linux\")] fn linux() {} fn keep() {}",
            cfg!(target_os = "linux"),
        ),
        ("#[cfg(windows)] fn w() {} fn keep() {}", cfg!(windows)),
        (
            "#[cfg(debug_assertions)] fn f() {} fn keep() {}",
            cfg!(debug_assertions),
        ),
        (
            "#[cfg(not(debug_assertions))] fn f() {} fn keep() {}",
            cfg!(not(debug_assertions)),
        ),
    ] {
        assert_gated_or_keep(&cyclo(src), gated_present);
    }
}
